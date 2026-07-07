// @ts-check

const { themes } = require('prism-react-renderer');

const config = {
  title: 'Neoism',
  tagline: 'A terminal-first Neovim IDE with a local agent runtime.',
  favicon: '/assets/neoism-wordmark.svg',
  url: 'https://neoism.dev',
  trailingSlash: false,
  baseUrl: '/',
  organizationName: 'parkersettle',
  projectName: 'neoism',
  onBrokenLinks: 'throw',
  markdown: {
    hooks: {
      onBrokenMarkdownLinks: 'warn',
    },
  },
  i18n: {
    defaultLocale: 'en',
    locales: ['en'],
  },
  presets: [
    [
      'classic',
      /** @type {import('@docusaurus/preset-classic').Options} */
      ({
        docs: {
          sidebarPath: require.resolve('./sidebars.js'),
          editUrl: 'https://github.com/parkersettle/neoism/tree/main/docs/',
          routeBasePath: 'docs',
        },
        blog: false,
        theme: {
          customCss: [require.resolve('./src/css/custom.css')],
        },
      }),
    ],
  ],
  themeConfig:
    /** @type {import('@docusaurus/preset-classic').ThemeConfig} */
    ({
      image: '/assets/neoism-wordmark.png',
      navbar: {
        style: 'dark',
        logo: {
          alt: 'Neoism wordmark',
          src: '/assets/neoism-wordmark.png',
        },
        items: [
          { to: '/docs/intro', label: 'Docs', position: 'left' },
          { to: '/docs/install', label: 'Install', position: 'left' },
          { to: '/docs/agent/overview', label: 'Agent', position: 'left' },
          { to: '/docs/development/overview', label: 'Development', position: 'left' },
          { to: '/contributing', label: 'Contributing', position: 'left' },
          {
            href: 'https://github.com/parkersettle/neoism',
            position: 'right',
            className: 'header-github-link',
            'aria-label': 'GitHub repository',
          },
        ],
      },
      footer: {
        style: 'dark',
        links: [
          {
            title: 'Docs',
            items: [
              { label: 'What is Neoism?', to: '/docs/intro' },
              { label: 'Install', to: '/docs/install' },
              { label: 'Using Neoism', to: '/docs/using-neoism/overview' },
              { label: 'Configuration', to: '/docs/configuration/overview' },
            ],
          },
          {
            title: 'Agent',
            items: [
              { label: 'Overview', to: '/docs/agent/overview' },
              { label: 'CLI', to: '/docs/agent/cli' },
              { label: 'Tools and permissions', to: '/docs/agent/tools-and-permissions' },
            ],
          },
          {
            title: 'Project',
            items: [
              { label: 'Development', to: '/docs/development/overview' },
              { label: 'Repository map', to: '/docs/reference/repo-map' },
              { label: 'GitHub', href: 'https://github.com/parkersettle/neoism' },
            ],
          },
        ],
        copyright: `Copyright © ${new Date().getFullYear()} Neoism contributors.`,
      },
      prism: {
        theme: themes.dracula,
        darkTheme: themes.dracula,
        additionalLanguages: ['bash', 'toml', 'nix', 'rust'],
      },
      colorMode: {
        defaultMode: 'dark',
        disableSwitch: false,
        respectPrefersColorScheme: false,
      },
    }),
};

module.exports = config;