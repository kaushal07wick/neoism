// @ts-check

/** @type {import('@docusaurus/plugin-content-docs').SidebarsConfig} */
const sidebars = {
  docs: [
    'intro',
    {
      type: 'category',
      label: 'Install',
      collapsed: false,
      items: ['install/index', 'install/build-from-source'],
    },
    {
      type: 'category',
      label: 'Using Neoism',
      collapsed: false,
      items: ['using-neoism/overview', 'using-neoism/agent-panel'],
    },
    {
      type: 'category',
      label: 'Configuration',
      collapsed: false,
      items: [
        'configuration/overview',
        'configuration/keybindings',
        'configuration/terminal',
      ],
    },
    {
      type: 'category',
      label: 'Agent',
      collapsed: false,
      items: ['agent/overview', 'agent/cli', 'agent/tools-and-permissions'],
    },
    {
      type: 'category',
      label: 'Development',
      collapsed: false,
      items: ['development/overview', 'development/docs'],
    },
    {
      type: 'category',
      label: 'Reference',
      collapsed: false,
      items: ['reference/cli', 'reference/repo-map'],
    },
  ],
};

module.exports = sidebars;