// @ts-check

import Link from '@docusaurus/Link';
import useDocusaurusContext from '@docusaurus/useDocusaurusContext';
import Heading from '@theme/Heading';
import Layout from '@theme/Layout';
import clsx from 'clsx';

import styles from './index.module.css';

const highlights = [
  {
    title: 'Terminal-first Neovim IDE',
    body: 'Neoism keeps the terminal as the workspace and adds Rust-owned IDE chrome around managed Neovim.',
  },
  {
    title: 'Local agent runtime',
    body: 'Run workspace-aware agent sessions, tools, permissions, provider streams, and subagents from a local Rust server.',
  },
  {
    title: 'Native Rust workspace',
    body: 'No Electron shell: the desktop app, terminal core, shared UI state, daemon services, protocol types, and agent runtime live in one Rust workspace.',
  },
  {
    title: 'Shared desktop and web architecture',
    body: 'Desktop owns native windows and PTYs; web talks to the workspace daemon through protocol snapshots instead of cloning platform policy.',
  },
];

function HomepageHeader() {
  const { siteConfig } = useDocusaurusContext();

  return (
    <header className={clsx('container', styles.header)}>
      <div className={styles.headerText}>
        <p className={styles.eyebrow}>Neoism Documentation</p>
        <Heading as="h1" className={styles.title}>
          {siteConfig.title}
        </Heading>
        <p className={styles.subtitle}>{siteConfig.tagline}</p>
        <p className={styles.tagline}>
          A native terminal-first IDE for Neovim users, workspace agents, and fast local development.
        </p>
        <div className={styles.actions}>
          <Link to="/docs/intro" className={styles.actionButton}>
            Read the docs
          </Link>
          <Link to="/docs/install" className={styles.actionButtonSecondary}>
            Build from source
          </Link>
        </div>
      </div>
      <div className={styles.heroCard}>
        <div className={styles.terminalChrome}>
          <span />
          <span />
          <span />
        </div>
        <pre className={styles.terminalText}>{`$ cargo run -p neoism
$ cargo run -p neoism-agent -- serve

Neoism workspace ready
agent: http://127.0.0.1:4096`}</pre>
      </div>
    </header>
  );
}

function Highlights() {
  return (
    <section className={clsx('container', styles.highlights)}>
      {highlights.map((highlight) => (
        <article key={highlight.title} className={styles.highlightCard}>
          <Heading as="h2">{highlight.title}</Heading>
          <p>{highlight.body}</p>
        </article>
      ))}
    </section>
  );
}

export default function Home() {
  return (
    <Layout
      title="Neoism Docs"
      description="Documentation for the Neoism terminal-first Neovim IDE and local agent runtime."
    >
      <HomepageHeader />
      <main>
        <Highlights />
      </main>
    </Layout>
  );
}