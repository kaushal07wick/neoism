use std::collections::VecDeque;

const OUTPUT_HISTORY_LIMIT: usize = 2 * 1024 * 1024;

#[derive(Clone, Debug)]
pub(super) struct PtyOutputChunk {
    pub(super) data: String,
    pub(super) cursor: u64,
}

#[derive(Clone, Debug)]
pub(super) enum PtyOutputEvent {
    Data(PtyOutputChunk),
    Exited,
}

#[derive(Debug)]
pub(super) struct BufferedChunk {
    start: u64,
    end: u64,
    data: String,
}

#[derive(Debug, Default)]
pub(super) struct PtyOutputBuffer {
    chunks: VecDeque<BufferedChunk>,
    cursor: u64,
    bytes: usize,
}

impl PtyOutputBuffer {
    pub(super) fn cursor(&self) -> u64 {
        self.cursor
    }

    pub(super) fn push(&mut self, data: String) -> PtyOutputChunk {
        let start = self.cursor;
        let width = utf16_len(&data);
        let end = start.saturating_add(width);
        self.bytes = self.bytes.saturating_add(data.len());
        self.cursor = end;
        self.chunks.push_back(BufferedChunk {
            start,
            end,
            data: data.clone(),
        });
        while self.bytes > OUTPUT_HISTORY_LIMIT {
            let Some(chunk) = self.chunks.pop_front() else {
                break;
            };
            self.bytes = self.bytes.saturating_sub(chunk.data.len());
        }
        PtyOutputChunk { data, cursor: end }
    }

    pub(super) fn replay_from(&self, cursor: u64) -> Vec<PtyOutputChunk> {
        self.chunks
            .iter()
            .filter_map(|chunk| {
                if chunk.end <= cursor {
                    return None;
                }
                if cursor <= chunk.start {
                    return Some(PtyOutputChunk {
                        data: chunk.data.clone(),
                        cursor: chunk.end,
                    });
                }
                let data =
                    substring_from_utf16(&chunk.data, cursor.saturating_sub(chunk.start));
                (!data.is_empty()).then_some(PtyOutputChunk {
                    data,
                    cursor: chunk.end,
                })
            })
            .collect()
    }
}
fn utf16_len(value: &str) -> u64 {
    value.encode_utf16().count() as u64
}

fn substring_from_utf16(value: &str, offset: u64) -> String {
    if offset == 0 {
        return value.to_string();
    }

    let mut consumed = 0;
    for (index, ch) in value.char_indices() {
        if consumed >= offset {
            return value[index..].to_string();
        }
        consumed += ch.len_utf16() as u64;
    }
    String::new()
}
