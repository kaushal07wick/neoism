use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Row, SqlitePool};

use super::config::NeoismWorkspace;
use super::notes::{
    BlockEntry, HeadingEntry, LinkEntry, LinkKind, NoteEntry, PropertyEntry,
    WorkspaceNoteIndex,
};

const SCHEMA_VERSION: i64 = 4;

pub fn workspace_graph_db_path(workspace: &NeoismWorkspace) -> PathBuf {
    workspace.cache_dir().join("notes.sqlite")
}

pub fn rebuild_note_graph(
    workspace: &NeoismWorkspace,
    index: &WorkspaceNoteIndex,
) -> std::io::Result<()> {
    block_on_db(rebuild_note_graph_async(workspace, index))
}

pub fn replace_note_graph_file(
    workspace: &NeoismWorkspace,
    path: impl AsRef<Path>,
) -> std::io::Result<()> {
    block_on_db(replace_note_graph_file_async(workspace, path.as_ref()))
}

pub fn remove_note_graph_file(
    workspace: &NeoismWorkspace,
    path: impl AsRef<Path>,
) -> std::io::Result<()> {
    block_on_db(remove_note_graph_file_async(workspace, path.as_ref()))
}

async fn replace_note_graph_file_async(
    workspace: &NeoismWorkspace,
    path: &Path,
) -> std::io::Result<()> {
    let index = match WorkspaceNoteIndex::build_file(workspace, path)? {
        Some(index) => index,
        None => return remove_note_graph_file_async(workspace, path).await,
    };
    let Some(note) = index.notes.first() else {
        return Ok(());
    };
    let pool = open_pool(&workspace_graph_db_path(workspace)).await?;
    migrate(&pool).await?;
    delete_note_rows(&pool, &note.relative_path).await?;
    insert_note_index(&pool, workspace, &index, now_unix_seconds(), false).await?;
    refresh_link_targets(&pool).await?;
    pool.close().await;
    Ok(())
}

async fn remove_note_graph_file_async(
    workspace: &NeoismWorkspace,
    path: &Path,
) -> std::io::Result<()> {
    let absolute = workspace.resolve_note_path(path);
    let relative_path = workspace.note_path_label(&absolute);
    let pool = open_pool(&workspace_graph_db_path(workspace)).await?;
    migrate(&pool).await?;
    delete_note_rows(&pool, &relative_path).await?;
    refresh_link_targets(&pool).await?;
    pool.close().await;
    Ok(())
}

async fn rebuild_note_graph_async(
    workspace: &NeoismWorkspace,
    index: &WorkspaceNoteIndex,
) -> std::io::Result<()> {
    let pool = open_pool(&workspace_graph_db_path(workspace)).await?;
    migrate(&pool).await?;
    sqlx::query("DELETE FROM blocks_fts")
        .execute(&pool)
        .await
        .map_err(io_other)?;
    sqlx::query("DELETE FROM links")
        .execute(&pool)
        .await
        .map_err(io_other)?;
    sqlx::query("DELETE FROM tags")
        .execute(&pool)
        .await
        .map_err(io_other)?;
    sqlx::query("DELETE FROM tasks")
        .execute(&pool)
        .await
        .map_err(io_other)?;
    sqlx::query("DELETE FROM note_properties")
        .execute(&pool)
        .await
        .map_err(io_other)?;
    sqlx::query("DELETE FROM headings")
        .execute(&pool)
        .await
        .map_err(io_other)?;
    sqlx::query("DELETE FROM blocks")
        .execute(&pool)
        .await
        .map_err(io_other)?;
    sqlx::query("DELETE FROM notes")
        .execute(&pool)
        .await
        .map_err(io_other)?;

    let indexed_at = now_unix_seconds();
    let note_ids = note_ids(&workspace.config.id, &index.notes);
    let block_ids = block_ids(&workspace.config.id, &note_ids, &index.blocks);
    let source_block_by_line = source_block_by_line(&block_ids, &index.blocks);
    let heading_target_ids = heading_target_ids(
        &workspace.config.id,
        &note_ids,
        &source_block_by_line,
        &index.headings,
    );

    for note in &index.notes {
        let Some(note_id) = note_ids.get(&note.relative_path) else {
            continue;
        };
        sqlx::query(
            r#"
            INSERT INTO notes (
                id, workspace_id, path, title, modified, content_hash, indexed_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(note_id)
        .bind(&workspace.config.id)
        .bind(&note.relative_path)
        .bind(&note.title)
        .bind(note.modified)
        .bind(stable_hash_hex(&format!(
            "note-content:{}:{}",
            note.relative_path, note.hash
        )))
        .bind(indexed_at)
        .execute(&pool)
        .await
        .map_err(io_other)?;
    }

    for property in &index.properties {
        let Some(note_id) = note_ids.get(&property.note_path) else {
            continue;
        };
        insert_note_property(&pool, &workspace.config.id, note_id, property).await?;
    }

    for block in &index.blocks {
        let Some(note_id) = note_ids.get(&block.note_path) else {
            continue;
        };
        let Some(block_id) = block_ids.get(&block_key(block)) else {
            continue;
        };
        sqlx::query(
            r#"
            INSERT INTO blocks (
                id, note_id, path, kind, start_line, end_line, ordinal, anchor, text, text_hash
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(block_id)
        .bind(note_id)
        .bind(&block.note_path)
        .bind(block.kind.as_str())
        .bind(block.start_line as i64)
        .bind(block.end_line as i64)
        .bind(block.ordinal as i64)
        .bind(&block.anchor)
        .bind(&block.text)
        .bind(stable_hash_hex(&block.text))
        .execute(&pool)
        .await
        .map_err(io_other)?;
        insert_block_identity(&pool, block_id, block, now_unix_seconds()).await?;
        sqlx::query(
            r#"
            INSERT INTO blocks_fts (
                block_id, note_id, path, kind, text
            ) VALUES (?, ?, ?, ?, ?)
            "#,
        )
        .bind(block_id)
        .bind(note_id)
        .bind(&block.note_path)
        .bind(block.kind.as_str())
        .bind(&block.text)
        .execute(&pool)
        .await
        .map_err(io_other)?;
    }

    for heading in &index.headings {
        let Some(note_id) = note_ids.get(&heading.note_path) else {
            continue;
        };
        let Some(block_id) =
            source_block_by_line.get(&(heading.note_path.clone(), heading.line))
        else {
            continue;
        };
        let heading_id = heading_entity_id(&workspace.config.id, block_id, heading);
        sqlx::query(
            r#"
            INSERT INTO headings (
                id, block_id, note_id, path, line, level, text, slug
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(heading_id)
        .bind(block_id)
        .bind(note_id)
        .bind(&heading.note_path)
        .bind(heading.line as i64)
        .bind(heading.level as i64)
        .bind(&heading.text)
        .bind(&heading.slug)
        .execute(&pool)
        .await
        .map_err(io_other)?;
    }

    for link in &index.links {
        let Some(source_note_id) = note_ids.get(&link.source_path) else {
            continue;
        };
        let source_block_id = source_block_by_line
            .get(&(link.source_path.clone(), link.source_line))
            .cloned();
        let target_note = resolve_target_note(index, link);
        let target_note_id = target_note
            .and_then(|note| note_ids.get(&note.relative_path))
            .cloned();
        let target_heading_id = target_note
            .and_then(|note| link.heading.as_deref().map(|heading| (note, heading)))
            .and_then(|(note, heading)| {
                let slug = heading_slug(heading);
                heading_target_ids
                    .get(&heading_key(&note.relative_path, &slug))
                    .cloned()
            });
        sqlx::query(
            r#"
            INSERT INTO links (
                id, source_block_id, source_note_id, source_path, source_line, raw,
                target, target_note_id, heading, target_heading_id, alias, kind
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(stable_entity_id(
            "link",
            &workspace.config.id,
            &format!(
                "{}:{}:{}:{}",
                link.source_path, link.source_line, link.raw, link.target
            ),
        ))
        .bind(source_block_id)
        .bind(source_note_id)
        .bind(&link.source_path)
        .bind(link.source_line as i64)
        .bind(&link.raw)
        .bind(&link.target)
        .bind(target_note_id)
        .bind(&link.heading)
        .bind(target_heading_id)
        .bind(&link.alias)
        .bind(link_kind_name(&link.kind))
        .execute(&pool)
        .await
        .map_err(io_other)?;
    }

    for tag in &index.tags {
        let Some(note_id) = note_ids.get(&tag.note_path) else {
            continue;
        };
        let block_id = source_block_by_line
            .get(&(tag.note_path.clone(), tag.line))
            .cloned();
        sqlx::query(
            r#"
            INSERT INTO tags (
                id, block_id, note_id, path, line, tag
            ) VALUES (?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(stable_entity_id(
            "tag",
            &workspace.config.id,
            &format!("{}:{}:{}", tag.note_path, tag.line, tag.tag),
        ))
        .bind(block_id)
        .bind(note_id)
        .bind(&tag.note_path)
        .bind(tag.line as i64)
        .bind(&tag.tag)
        .execute(&pool)
        .await
        .map_err(io_other)?;
    }

    for task in &index.tasks {
        let Some(note_id) = note_ids.get(&task.note_path) else {
            continue;
        };
        let block_id = source_block_by_line
            .get(&(task.note_path.clone(), task.line))
            .cloned();
        sqlx::query(
            r#"
            INSERT INTO tasks (
                id, block_id, note_id, path, line, checked, text
            ) VALUES (?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(stable_entity_id(
            "task",
            &workspace.config.id,
            &format!("{}:{}:{}", task.note_path, task.line, task.text),
        ))
        .bind(block_id)
        .bind(note_id)
        .bind(&task.note_path)
        .bind(task.line as i64)
        .bind(if task.checked { 1_i64 } else { 0_i64 })
        .bind(&task.text)
        .execute(&pool)
        .await
        .map_err(io_other)?;
    }

    pool.close().await;
    Ok(())
}

async fn insert_note_index(
    pool: &SqlitePool,
    workspace: &NeoismWorkspace,
    index: &WorkspaceNoteIndex,
    indexed_at: i64,
    resolve_links_from_index: bool,
) -> std::io::Result<()> {
    let note_ids = note_ids(&workspace.config.id, &index.notes);
    let block_ids = block_ids(&workspace.config.id, &note_ids, &index.blocks);
    let source_block_by_line = source_block_by_line(&block_ids, &index.blocks);
    let heading_target_ids = heading_target_ids(
        &workspace.config.id,
        &note_ids,
        &source_block_by_line,
        &index.headings,
    );

    for note in &index.notes {
        let Some(note_id) = note_ids.get(&note.relative_path) else {
            continue;
        };
        sqlx::query(
            r#"
            INSERT INTO notes (
                id, workspace_id, path, title, modified, content_hash, indexed_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(note_id)
        .bind(&workspace.config.id)
        .bind(&note.relative_path)
        .bind(&note.title)
        .bind(note.modified)
        .bind(stable_hash_hex(&format!(
            "note-content:{}:{}",
            note.relative_path, note.hash
        )))
        .bind(indexed_at)
        .execute(pool)
        .await
        .map_err(io_other)?;
    }

    for property in &index.properties {
        let Some(note_id) = note_ids.get(&property.note_path) else {
            continue;
        };
        insert_note_property(pool, &workspace.config.id, note_id, property).await?;
    }

    for block in &index.blocks {
        let Some(note_id) = note_ids.get(&block.note_path) else {
            continue;
        };
        let Some(block_id) = block_ids.get(&block_key(block)) else {
            continue;
        };
        sqlx::query(
            r#"
            INSERT INTO blocks (
                id, note_id, path, kind, start_line, end_line, ordinal, anchor, text, text_hash
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(block_id)
        .bind(note_id)
        .bind(&block.note_path)
        .bind(block.kind.as_str())
        .bind(block.start_line as i64)
        .bind(block.end_line as i64)
        .bind(block.ordinal as i64)
        .bind(&block.anchor)
        .bind(&block.text)
        .bind(stable_hash_hex(&block.text))
        .execute(pool)
        .await
        .map_err(io_other)?;
        insert_block_identity(pool, block_id, block, indexed_at).await?;
        sqlx::query(
            r#"
            INSERT INTO blocks_fts (
                block_id, note_id, path, kind, text
            ) VALUES (?, ?, ?, ?, ?)
            "#,
        )
        .bind(block_id)
        .bind(note_id)
        .bind(&block.note_path)
        .bind(block.kind.as_str())
        .bind(&block.text)
        .execute(pool)
        .await
        .map_err(io_other)?;
    }

    for heading in &index.headings {
        let Some(note_id) = note_ids.get(&heading.note_path) else {
            continue;
        };
        let Some(block_id) =
            source_block_by_line.get(&(heading.note_path.clone(), heading.line))
        else {
            continue;
        };
        let heading_id = heading_entity_id(&workspace.config.id, block_id, heading);
        sqlx::query(
            r#"
            INSERT INTO headings (
                id, block_id, note_id, path, line, level, text, slug
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(heading_id)
        .bind(block_id)
        .bind(note_id)
        .bind(&heading.note_path)
        .bind(heading.line as i64)
        .bind(heading.level as i64)
        .bind(&heading.text)
        .bind(&heading.slug)
        .execute(pool)
        .await
        .map_err(io_other)?;
    }

    for link in &index.links {
        let Some(source_note_id) = note_ids.get(&link.source_path) else {
            continue;
        };
        let source_block_id = source_block_by_line
            .get(&(link.source_path.clone(), link.source_line))
            .cloned();
        let target_note = resolve_links_from_index
            .then(|| resolve_target_note(index, link))
            .flatten();
        let target_note_id = target_note
            .and_then(|note| note_ids.get(&note.relative_path))
            .cloned();
        let target_heading_id = target_note
            .and_then(|note| link.heading.as_deref().map(|heading| (note, heading)))
            .and_then(|(note, heading)| {
                let slug = heading_slug(heading);
                heading_target_ids
                    .get(&heading_key(&note.relative_path, &slug))
                    .cloned()
            });
        sqlx::query(
            r#"
            INSERT INTO links (
                id, source_block_id, source_note_id, source_path, source_line, raw,
                target, target_note_id, heading, target_heading_id, alias, kind
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(stable_entity_id(
            "link",
            &workspace.config.id,
            &format!(
                "{}:{}:{}:{}",
                link.source_path, link.source_line, link.raw, link.target
            ),
        ))
        .bind(source_block_id)
        .bind(source_note_id)
        .bind(&link.source_path)
        .bind(link.source_line as i64)
        .bind(&link.raw)
        .bind(&link.target)
        .bind(target_note_id)
        .bind(&link.heading)
        .bind(target_heading_id)
        .bind(&link.alias)
        .bind(link_kind_name(&link.kind))
        .execute(pool)
        .await
        .map_err(io_other)?;
    }

    for tag in &index.tags {
        let Some(note_id) = note_ids.get(&tag.note_path) else {
            continue;
        };
        let block_id = source_block_by_line
            .get(&(tag.note_path.clone(), tag.line))
            .cloned();
        sqlx::query(
            r#"
            INSERT INTO tags (
                id, block_id, note_id, path, line, tag
            ) VALUES (?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(stable_entity_id(
            "tag",
            &workspace.config.id,
            &format!("{}:{}:{}", tag.note_path, tag.line, tag.tag),
        ))
        .bind(block_id)
        .bind(note_id)
        .bind(&tag.note_path)
        .bind(tag.line as i64)
        .bind(&tag.tag)
        .execute(pool)
        .await
        .map_err(io_other)?;
    }

    for task in &index.tasks {
        let Some(note_id) = note_ids.get(&task.note_path) else {
            continue;
        };
        let block_id = source_block_by_line
            .get(&(task.note_path.clone(), task.line))
            .cloned();
        sqlx::query(
            r#"
            INSERT INTO tasks (
                id, block_id, note_id, path, line, checked, text
            ) VALUES (?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(stable_entity_id(
            "task",
            &workspace.config.id,
            &format!("{}:{}:{}", task.note_path, task.line, task.text),
        ))
        .bind(block_id)
        .bind(note_id)
        .bind(&task.note_path)
        .bind(task.line as i64)
        .bind(if task.checked { 1_i64 } else { 0_i64 })
        .bind(&task.text)
        .execute(pool)
        .await
        .map_err(io_other)?;
    }

    Ok(())
}

async fn insert_note_property(
    pool: &SqlitePool,
    workspace_id: &str,
    note_id: &str,
    property: &PropertyEntry,
) -> std::io::Result<()> {
    sqlx::query(
        r#"
        INSERT INTO note_properties (
            id, note_id, path, key, value, value_type
        ) VALUES (?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(stable_entity_id(
        "property",
        workspace_id,
        &format!("{}:{}", property.note_path, property.key),
    ))
    .bind(note_id)
    .bind(&property.note_path)
    .bind(&property.key)
    .bind(&property.value)
    .bind(&property.value_type)
    .execute(pool)
    .await
    .map_err(io_other)?;
    Ok(())
}

async fn insert_block_identity(
    pool: &SqlitePool,
    block_id: &str,
    block: &BlockEntry,
    updated_at: i64,
) -> std::io::Result<()> {
    sqlx::query(
        r#"
        INSERT INTO block_identity (
            id, path, kind, ordinal, anchor, text_hash, updated_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?)
        ON CONFLICT(id) DO UPDATE SET
            path = excluded.path,
            kind = excluded.kind,
            ordinal = excluded.ordinal,
            anchor = excluded.anchor,
            text_hash = excluded.text_hash,
            updated_at = excluded.updated_at
        "#,
    )
    .bind(block_id)
    .bind(&block.note_path)
    .bind(block.kind.as_str())
    .bind(block.ordinal as i64)
    .bind(&block.anchor)
    .bind(stable_hash_hex(&block.text))
    .bind(updated_at)
    .execute(pool)
    .await
    .map_err(io_other)?;
    Ok(())
}

async fn delete_note_rows(pool: &SqlitePool, relative_path: &str) -> std::io::Result<()> {
    sqlx::query("DELETE FROM blocks_fts WHERE path = ?")
        .bind(relative_path)
        .execute(pool)
        .await
        .map_err(io_other)?;
    sqlx::query("DELETE FROM notes WHERE path = ?")
        .bind(relative_path)
        .execute(pool)
        .await
        .map_err(io_other)?;
    Ok(())
}

#[derive(Debug, Clone)]
struct DbNote {
    id: String,
    path: String,
    title: String,
}

#[derive(Debug, Clone)]
struct DbHeading {
    id: String,
    note_id: String,
    path: String,
    slug: String,
}

async fn refresh_link_targets(pool: &SqlitePool) -> std::io::Result<()> {
    let note_rows = sqlx::query("SELECT id, path, title FROM notes")
        .fetch_all(pool)
        .await
        .map_err(io_other)?;
    let notes = note_rows
        .into_iter()
        .map(|row| DbNote {
            id: row.get("id"),
            path: row.get("path"),
            title: row.get("title"),
        })
        .collect::<Vec<_>>();
    let heading_rows = sqlx::query("SELECT id, note_id, path, slug FROM headings")
        .fetch_all(pool)
        .await
        .map_err(io_other)?;
    let headings = heading_rows
        .into_iter()
        .map(|row| DbHeading {
            id: row.get("id"),
            note_id: row.get("note_id"),
            path: row.get("path"),
            slug: row.get("slug"),
        })
        .collect::<Vec<_>>();
    let link_rows = sqlx::query(
        "SELECT id, source_path, target, heading, kind FROM links ORDER BY source_path, source_line",
    )
    .fetch_all(pool)
    .await
    .map_err(io_other)?;

    for row in link_rows {
        let id: String = row.get("id");
        let source_path: String = row.get("source_path");
        let target: String = row.get("target");
        let heading: Option<String> = row.get("heading");
        let kind: String = row.get("kind");
        let target_note = resolve_db_target_note(&notes, &source_path, &target, &kind);
        let target_note_id = target_note.map(|note| note.id.clone());
        let target_heading_id = target_note
            .and_then(|note| {
                heading.as_deref().map(heading_slug).and_then(|slug| {
                    resolve_db_heading(&headings, &note.id, &note.path, &slug)
                })
            })
            .map(|heading| heading.id.clone());
        sqlx::query(
            "UPDATE links SET target_note_id = ?, target_heading_id = ? WHERE id = ?",
        )
        .bind(target_note_id)
        .bind(target_heading_id)
        .bind(id)
        .execute(pool)
        .await
        .map_err(io_other)?;
    }
    Ok(())
}

pub(crate) async fn open_pool(path: &Path) -> std::io::Result<SqlitePool> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true)
        .foreign_keys(true)
        .pragma("journal_mode", "WAL")
        .pragma("synchronous", "NORMAL");
    SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .map_err(io_other)
}

pub(crate) async fn migrate(pool: &SqlitePool) -> std::io::Result<()> {
    for query in SCHEMA {
        sqlx::query(query).execute(pool).await.map_err(io_other)?;
    }
    apply_schema_upgrades(pool).await?;
    let current: Option<i64> =
        sqlx::query_scalar("SELECT MAX(version) FROM schema_migrations")
            .fetch_one(pool)
            .await
            .map_err(io_other)?;
    if current.unwrap_or(0) < SCHEMA_VERSION {
        sqlx::query(
            "INSERT OR IGNORE INTO schema_migrations (version, applied_at) VALUES (?, ?)",
        )
        .bind(SCHEMA_VERSION)
        .bind(now_unix_seconds())
        .execute(pool)
        .await
        .map_err(io_other)?;
    }
    Ok(())
}

async fn apply_schema_upgrades(pool: &SqlitePool) -> std::io::Result<()> {
    let _ = sqlx::query("ALTER TABLE blocks ADD COLUMN anchor TEXT")
        .execute(pool)
        .await;
    Ok(())
}

const SCHEMA: &[&str] = &[
    r#"
    CREATE TABLE IF NOT EXISTS schema_migrations (
        version INTEGER PRIMARY KEY,
        applied_at INTEGER NOT NULL
    )
    "#,
    r#"
    CREATE TABLE IF NOT EXISTS notes (
        id TEXT PRIMARY KEY,
        workspace_id TEXT NOT NULL,
        path TEXT NOT NULL UNIQUE,
        title TEXT NOT NULL,
        modified INTEGER NOT NULL,
        content_hash TEXT NOT NULL,
        indexed_at INTEGER NOT NULL
    )
    "#,
    r#"
    CREATE TABLE IF NOT EXISTS blocks (
        id TEXT PRIMARY KEY,
        note_id TEXT NOT NULL,
        path TEXT NOT NULL,
        kind TEXT NOT NULL,
        start_line INTEGER NOT NULL,
        end_line INTEGER NOT NULL,
        ordinal INTEGER NOT NULL,
        anchor TEXT,
        text TEXT NOT NULL,
        text_hash TEXT NOT NULL,
        FOREIGN KEY(note_id) REFERENCES notes(id) ON DELETE CASCADE
    )
    "#,
    r#"
    CREATE VIRTUAL TABLE IF NOT EXISTS blocks_fts USING fts5(
        block_id UNINDEXED,
        note_id UNINDEXED,
        path UNINDEXED,
        kind UNINDEXED,
        text,
        tokenize = 'unicode61'
    )
    "#,
    r#"
    CREATE TABLE IF NOT EXISTS headings (
        id TEXT PRIMARY KEY,
        block_id TEXT NOT NULL,
        note_id TEXT NOT NULL,
        path TEXT NOT NULL,
        line INTEGER NOT NULL,
        level INTEGER NOT NULL,
        text TEXT NOT NULL,
        slug TEXT NOT NULL,
        FOREIGN KEY(block_id) REFERENCES blocks(id) ON DELETE CASCADE,
        FOREIGN KEY(note_id) REFERENCES notes(id) ON DELETE CASCADE
    )
    "#,
    r#"
    CREATE TABLE IF NOT EXISTS block_identity (
        id TEXT PRIMARY KEY,
        path TEXT NOT NULL,
        kind TEXT NOT NULL,
        ordinal INTEGER NOT NULL,
        anchor TEXT,
        text_hash TEXT NOT NULL,
        updated_at INTEGER NOT NULL
    )
    "#,
    r#"
    CREATE TABLE IF NOT EXISTS note_properties (
        id TEXT PRIMARY KEY,
        note_id TEXT NOT NULL,
        path TEXT NOT NULL,
        key TEXT NOT NULL,
        value TEXT NOT NULL,
        value_type TEXT NOT NULL,
        FOREIGN KEY(note_id) REFERENCES notes(id) ON DELETE CASCADE
    )
    "#,
    r#"
    CREATE TABLE IF NOT EXISTS links (
        id TEXT PRIMARY KEY,
        source_block_id TEXT,
        source_note_id TEXT NOT NULL,
        source_path TEXT NOT NULL,
        source_line INTEGER NOT NULL,
        raw TEXT NOT NULL,
        target TEXT NOT NULL,
        target_note_id TEXT,
        heading TEXT,
        target_heading_id TEXT,
        alias TEXT,
        kind TEXT NOT NULL,
        FOREIGN KEY(source_block_id) REFERENCES blocks(id) ON DELETE SET NULL,
        FOREIGN KEY(source_note_id) REFERENCES notes(id) ON DELETE CASCADE,
        FOREIGN KEY(target_note_id) REFERENCES notes(id) ON DELETE SET NULL,
        FOREIGN KEY(target_heading_id) REFERENCES headings(id) ON DELETE SET NULL
    )
    "#,
    r#"
    CREATE TABLE IF NOT EXISTS tags (
        id TEXT PRIMARY KEY,
        block_id TEXT,
        note_id TEXT NOT NULL,
        path TEXT NOT NULL,
        line INTEGER NOT NULL,
        tag TEXT NOT NULL,
        FOREIGN KEY(block_id) REFERENCES blocks(id) ON DELETE SET NULL,
        FOREIGN KEY(note_id) REFERENCES notes(id) ON DELETE CASCADE
    )
    "#,
    r#"
    CREATE TABLE IF NOT EXISTS tasks (
        id TEXT PRIMARY KEY,
        block_id TEXT,
        note_id TEXT NOT NULL,
        path TEXT NOT NULL,
        line INTEGER NOT NULL,
        checked INTEGER NOT NULL,
        text TEXT NOT NULL,
        FOREIGN KEY(block_id) REFERENCES blocks(id) ON DELETE SET NULL,
        FOREIGN KEY(note_id) REFERENCES notes(id) ON DELETE CASCADE
    )
    "#,
    "CREATE INDEX IF NOT EXISTS idx_blocks_note_line ON blocks(note_id, start_line, end_line)",
    "CREATE INDEX IF NOT EXISTS idx_headings_note_slug ON headings(note_id, slug)",
    "CREATE INDEX IF NOT EXISTS idx_links_source_note ON links(source_note_id)",
    "CREATE INDEX IF NOT EXISTS idx_links_target_note ON links(target_note_id)",
    "CREATE INDEX IF NOT EXISTS idx_links_unresolved ON links(target_note_id, kind)",
    "CREATE INDEX IF NOT EXISTS idx_tags_tag ON tags(tag)",
    "CREATE INDEX IF NOT EXISTS idx_tasks_checked ON tasks(checked)",
    "CREATE INDEX IF NOT EXISTS idx_block_identity_path ON block_identity(path, ordinal)",
    "CREATE INDEX IF NOT EXISTS idx_block_identity_anchor ON block_identity(path, anchor)",
    "CREATE INDEX IF NOT EXISTS idx_note_properties_key ON note_properties(key)",
];

fn note_ids(workspace_id: &str, notes: &[NoteEntry]) -> HashMap<String, String> {
    notes
        .iter()
        .map(|note| {
            (
                note.relative_path.clone(),
                stable_entity_id("note", workspace_id, &note.relative_path),
            )
        })
        .collect()
}

fn block_ids(
    workspace_id: &str,
    note_ids: &HashMap<String, String>,
    blocks: &[BlockEntry],
) -> HashMap<String, String> {
    blocks
        .iter()
        .filter_map(|block| {
            let note_id = note_ids.get(&block.note_path)?;
            let key = block_key(block);
            Some((
                key.clone(),
                stable_entity_id(
                    "block",
                    workspace_id,
                    &block_identity_key(note_id, block),
                ),
            ))
        })
        .collect()
}

fn block_identity_key(note_id: &str, block: &BlockEntry) -> String {
    if let Some(anchor) = &block.anchor {
        format!("{note_id}:anchor:{anchor}")
    } else {
        format!("{note_id}:{}:{}", block.kind.as_str(), block.ordinal)
    }
}

fn heading_target_ids(
    workspace_id: &str,
    note_ids: &HashMap<String, String>,
    source_block_by_line: &HashMap<(String, usize), String>,
    headings: &[HeadingEntry],
) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for heading in headings {
        if !note_ids.contains_key(&heading.note_path) {
            continue;
        }
        let Some(block_id) =
            source_block_by_line.get(&(heading.note_path.clone(), heading.line))
        else {
            continue;
        };
        let key = heading_key(&heading.note_path, &heading.slug);
        out.entry(key)
            .or_insert_with(|| heading_entity_id(workspace_id, block_id, heading));
    }
    out
}

fn heading_entity_id(
    workspace_id: &str,
    block_id: &str,
    heading: &HeadingEntry,
) -> String {
    stable_entity_id(
        "heading",
        workspace_id,
        &format!("{block_id}:{}", heading.slug),
    )
}

fn source_block_by_line(
    block_ids: &HashMap<String, String>,
    blocks: &[BlockEntry],
) -> HashMap<(String, usize), String> {
    let mut out = HashMap::new();
    for block in blocks {
        let Some(block_id) = block_ids.get(&block_key(block)) else {
            continue;
        };
        for line in block.start_line..=block.end_line {
            out.insert((block.note_path.clone(), line), block_id.clone());
        }
    }
    out
}

fn block_key(block: &BlockEntry) -> String {
    format!(
        "{}:{}:{}:{}",
        block.note_path, block.ordinal, block.start_line, block.end_line
    )
}

fn heading_key(note_path: &str, slug: &str) -> String {
    format!("{note_path}#{slug}")
}

fn resolve_target_note<'a>(
    index: &'a WorkspaceNoteIndex,
    link: &LinkEntry,
) -> Option<&'a NoteEntry> {
    if matches!(link.kind, LinkKind::CodeRef) {
        return None;
    }
    let source_dir = Path::new(&link.source_path)
        .parent()
        .unwrap_or_else(|| Path::new(""));
    let target = Path::new(&link.target);
    let mut candidates = Vec::new();
    if target.is_absolute() {
        candidates.push(relative_components(target));
    } else {
        candidates.push(normalize_relative_components(&source_dir.join(target)));
        candidates.push(relative_components(target));
    }
    if target.extension().is_none() {
        let base_candidates = candidates.clone();
        for candidate in base_candidates {
            for ext in ["md", "markdown", "mdx"] {
                candidates.push(format!("{candidate}.{ext}"));
            }
        }
    }
    index.notes.iter().find(|note| {
        candidates.iter().any(|candidate| {
            candidate == &note.relative_path
                || strip_markdown_extension(candidate)
                    == strip_markdown_extension(&note.relative_path)
        }) || note.title.eq_ignore_ascii_case(&link.target)
    })
}

fn resolve_db_target_note<'a>(
    notes: &'a [DbNote],
    source_path: &str,
    target: &str,
    kind: &str,
) -> Option<&'a DbNote> {
    if kind == "code_ref" {
        return None;
    }
    let source_dir = Path::new(source_path)
        .parent()
        .unwrap_or_else(|| Path::new(""));
    let target_path = Path::new(target);
    let mut candidates = Vec::new();
    if target_path.is_absolute() {
        candidates.push(relative_components(target_path));
    } else {
        candidates.push(normalize_relative_components(&source_dir.join(target_path)));
        candidates.push(relative_components(target_path));
    }
    if target_path.extension().is_none() {
        let base_candidates = candidates.clone();
        for candidate in base_candidates {
            for ext in ["md", "markdown", "mdx"] {
                candidates.push(format!("{candidate}.{ext}"));
            }
        }
    }
    notes.iter().find(|note| {
        candidates.iter().any(|candidate| {
            candidate == &note.path
                || strip_markdown_extension(candidate)
                    == strip_markdown_extension(&note.path)
        }) || note.title.eq_ignore_ascii_case(target)
    })
}

fn resolve_db_heading<'a>(
    headings: &'a [DbHeading],
    note_id: &str,
    note_path: &str,
    slug: &str,
) -> Option<&'a DbHeading> {
    headings
        .iter()
        .find(|heading| {
            heading.note_id == note_id
                && heading.path == note_path
                && heading.slug == slug
        })
        .or_else(|| {
            headings
                .iter()
                .find(|heading| heading.note_id == note_id && heading.slug == slug)
        })
}

fn link_kind_name(kind: &LinkKind) -> &'static str {
    match kind {
        LinkKind::Note => "note",
        LinkKind::CodeRef => "code_ref",
        LinkKind::Embed => "embed",
    }
}

fn heading_slug(text: &str) -> String {
    let mut slug = String::new();
    let mut last_dash = false;
    for ch in text.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash {
            slug.push('-');
            last_dash = true;
        }
    }
    slug.trim_matches('-').to_string()
}

fn stable_entity_id(kind: &str, workspace_id: &str, value: &str) -> String {
    format!(
        "{kind}:{}",
        stable_hash_hex(&format!("{workspace_id}:{value}"))
    )
}

fn stable_hash_hex(value: &str) -> String {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in value.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

fn relative_components(path: &Path) -> String {
    path.components()
        .filter_map(|component| match component {
            Component::Normal(part) => Some(part.to_string_lossy().into_owned()),
            Component::ParentDir => Some("..".to_string()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

fn normalize_relative_components(path: &Path) -> String {
    let mut parts = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => parts.push(part.to_string_lossy().into_owned()),
            Component::ParentDir => {
                parts.pop();
            }
            _ => {}
        }
    }
    parts.join("/")
}

fn strip_markdown_extension(path: &str) -> &str {
    for suffix in [".markdown", ".mdx", ".md"] {
        if let Some(stripped) = path.strip_suffix(suffix) {
            return stripped;
        }
    }
    path
}

fn now_unix_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

pub(crate) fn block_on_db<F, T>(future: F) -> std::io::Result<T>
where
    F: std::future::Future<Output = std::io::Result<T>> + Send,
    T: Send,
{
    if tokio::runtime::Handle::try_current().is_ok() {
        return std::thread::scope(|scope| {
            scope
                .spawn(move || block_on_db_runtime(future))
                .join()
                .map_err(|_| {
                    std::io::Error::other("note graph runtime thread panicked")
                })?
        });
    }
    block_on_db_runtime(future)
}

fn block_on_db_runtime<F, T>(future: F) -> std::io::Result<T>
where
    F: std::future::Future<Output = std::io::Result<T>>,
{
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(io_other)?
        .block_on(future)
}

pub(crate) fn io_other(err: impl std::fmt::Display) -> std::io::Error {
    std::io::Error::other(err.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{NeoismWorkspace, NotesConfig, WorkspaceConfig};

    fn test_workspace(name: &str) -> NeoismWorkspace {
        let root = std::env::temp_dir()
            .join(format!("neoism-graph-db-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join(".neoism/cache")).unwrap();
        let notes_root = root.join("Neoism/Vaults/Personal");
        NeoismWorkspace {
            root,
            config: WorkspaceConfig {
                version: 1,
                id: format!("test-{name}"),
                name: name.to_string(),
                notes: NotesConfig {
                    enabled: true,
                    workspace: notes_root.display().to_string(),
                    ignore: Vec::new(),
                },
            },
        }
    }

    #[test]
    fn rebuild_graph_persists_notes_blocks_links_tags_and_tasks() {
        let workspace = test_workspace("persist");
        std::fs::create_dir_all(workspace.notes_workspace_dir()).unwrap();
        std::fs::write(
            workspace.notes_workspace_dir().join("Roadmap.md"),
            "# Roadmap\n\n- [ ] ship #neoism\n\nSee [[Roadmap#Roadmap]]\n",
        )
        .unwrap();
        let index = WorkspaceNoteIndex::build(&workspace).unwrap();

        rebuild_note_graph(&workspace, &index).unwrap();
        let db_path = workspace_graph_db_path(&workspace);
        assert!(db_path.is_file());

        let counts = block_on_db(async {
            let pool = open_pool(&db_path).await?;
            migrate(&pool).await?;
            let notes: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM notes")
                .fetch_one(&pool)
                .await
                .map_err(io_other)?;
            let blocks: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM blocks")
                .fetch_one(&pool)
                .await
                .map_err(io_other)?;
            let links: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM links")
                .fetch_one(&pool)
                .await
                .map_err(io_other)?;
            let tags: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM tags")
                .fetch_one(&pool)
                .await
                .map_err(io_other)?;
            let tasks: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM tasks")
                .fetch_one(&pool)
                .await
                .map_err(io_other)?;
            let migration: i64 =
                sqlx::query_scalar("SELECT MAX(version) FROM schema_migrations")
                    .fetch_one(&pool)
                    .await
                    .map_err(io_other)?;
            pool.close().await;
            Ok((notes, blocks, links, tags, tasks, migration))
        })
        .unwrap();

        assert_eq!(counts.0, 1);
        assert!(counts.1 >= 3);
        assert_eq!(counts.2, 1);
        assert_eq!(counts.3, 1);
        assert_eq!(counts.4, 1);
        assert_eq!(counts.5, SCHEMA_VERSION);

        let _ = std::fs::remove_dir_all(&workspace.root);
    }

    #[test]
    fn rebuild_graph_allows_duplicate_heading_slugs() {
        let workspace = test_workspace("duplicate-headings");
        std::fs::create_dir_all(workspace.notes_workspace_dir()).unwrap();
        std::fs::write(
            workspace.notes_workspace_dir().join("Roadmap.md"),
            "# Roadmap\n\n## Tasks\n\nFirst section.\n\n## Tasks\n\nSecond section.\n\nSee [[Roadmap#Tasks]].\n",
        )
        .unwrap();
        let index = WorkspaceNoteIndex::build(&workspace).unwrap();

        rebuild_note_graph(&workspace, &index).unwrap();
        let db_path = workspace_graph_db_path(&workspace);

        let counts = block_on_db(async {
            let pool = open_pool(&db_path).await?;
            migrate(&pool).await?;
            let headings: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM headings")
                .fetch_one(&pool)
                .await
                .map_err(io_other)?;
            let distinct_heading_ids: i64 =
                sqlx::query_scalar("SELECT COUNT(DISTINCT id) FROM headings")
                    .fetch_one(&pool)
                    .await
                    .map_err(io_other)?;
            let resolved_heading_links: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM links WHERE target_heading_id IS NOT NULL",
            )
            .fetch_one(&pool)
            .await
            .map_err(io_other)?;
            pool.close().await;
            Ok((headings, distinct_heading_ids, resolved_heading_links))
        })
        .unwrap();

        assert_eq!(counts.0, 3);
        assert_eq!(counts.1, 3);
        assert_eq!(counts.2, 1);

        let _ = std::fs::remove_dir_all(&workspace.root);
    }
}
