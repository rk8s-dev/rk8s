const versions = require('./versions.json');
const {themes} = require('prism-react-renderer');
const lightTheme = themes.github;
const darkTheme = themes.dracula;

function getNextMinorVersionName() {
  const lastVersion = versions[0];
  let majorVersion = parseInt(lastVersion.split('.')[0]);
  let minorVersion = parseInt(lastVersion.split('.')[1]);
  if (majorVersion >= 1) {
    minorVersion += 1;
  } else {
    majorVersion = 0;
    minorVersion = 1;
  }
  return `${majorVersion}.${minorVersion}.x`;
}

/** @type {import('@docusaurus/types').DocusaurusConfig} */
module.exports = {
  title: 'RustyVault 🧰 A rusted vault that can do many awesome secrets management stuff...',
  tagline: '🧰 RustyVault is a modern secret management system, written in Rust. ',
  url: 'https://www.rustyvault.net',
  baseUrl: '/',
  onBrokenLinks: 'throw',
  onBrokenMarkdownLinks: 'throw',
  favicon: 'img/RV-logo-gold.png',
  organizationName: 'Tongsuo-Project',
  projectName: 'RustyVault',
  trailingSlash: true,
  i18n: {
    defaultLocale: 'en',
    locales: ['en', 'zh-CN'],
  },
  themeConfig: {
    colorMode: {
      defaultMode: 'light',
      disableSwitch: false,
      respectPrefersColorScheme: false,
    },
    image: 'img/RustyVault-arch.png',
    metadata: [
      {name: 'keywords', content: 'rust, hashicorp-vault, key-management, secure-storage, secrets-management, key-manager-service, secrets-manager, cloudnative-services'},
    ],
    navbar: {
      title: 'RustyVault',
      // logo: {
      //   alt: 'RustyVault Logo',
      //   src: 'img/RV-logo-gold.png',
      // },
      items: [
        {
          type: 'docSidebar',
          position: 'left',
          sidebarId: 'tutorialSidebar',
          label: 'Docs',
        },
        { 
          href: 'https://www.tongsuo.net/blog', 
          label: 'Blog', 
          position: 'left'
        },
        {
          to: 'https://crates.io/crates/rusty_vault',
          label: 'Crate',
          position: 'right',
        },
        {
          to: 'https://github.com/Tongsuo-Project/RustyVault',
          label: 'GitHub',
          position: 'right',
        },
        {
          type: 'docsVersionDropdown',
          position: 'right',
          dropdownActiveClassDisabled: true,
        },
        {
          type: 'localeDropdown',
          position: 'right',
        },
      ],
    },
    footer: {
      style: 'dark',
      links: [
        {
          title: 'Docs',
          items: [
            {
              label: 'Getting Started',
              to: '/docs/quick-start/',
            },
            {
              label: 'API Reference',
              to: 'https://docs.rs/rusty_vault/',
            },
          ],
        },
        {
          title: 'Community',
          items: [
            {
              label: 'GitHub',
              to: 'https://github.com/Tongsuo-Project/RustyVault/discussions',
            },
            {
              label: 'OSPP',
              to: 'https://summer-ospp.ac.cn/org/orgdetail/e4de262f-50b1-4f11-930b-8b8e841de420?lang=zh',
            },
          ],
        },
        {
          title: 'More',
          items: [
            // {
            //   label: 'Blog',
            //   to: '/blog/',
            // },
            {
              label: 'Tongsuo',
              to: 'https://tongsuo.net',
            },
          ],
        },
      ],
      copyright: `Copyright © 2021-${new Date().getFullYear()} OpenAtom Tongsuo Built with Docusaurus. <br/><a class="footer__link-item" href="https://beian.miit.gov.cn/">网站备案：京ICP备2020036654号-15</a> <img src="img/police.png" width="15" /> 京公网安备11030102011687号`,
    },
    prism: {
      additionalLanguages: [
        'toml',
        'rust',
        'bash',
        'json',
      ],
      theme: lightTheme,
      darkTheme: darkTheme,
    },
    announcementBar: {
      id: 'rustyvault-bar',
      content: 'If you like 🧰 RustyVault, please give us a <a target="_blank" href="https://github.com/Tongsuo-Project/RustyVault">⭐️ on GitHub</a>',
    },
  },
  themes: [
    [
      "@easyops-cn/docusaurus-search-local",
      /** @type {import("@easyops-cn/docusaurus-search-local").PluginOptions} */
      ({
        hashed: true,
        language: ["en"],
        highlightSearchTermsOnTargetPage: true,
        explicitSearchResultPath: true,
      }),
    ],
  ],
  presets: [
    [
      '@docusaurus/preset-classic',
      {
        docs: {
          sidebarPath: require.resolve('./sidebars.js'),
          editUrl: 'https://github.com/SeaQL/seaql.github.io/edit/master/SeaORM/',
          showLastUpdateAuthor: true,
          showLastUpdateTime: true,
          versions: {
            current: {
              label: `${getNextMinorVersionName()} 🚧`,
            },
          },
        },
        blog: false,
        theme: {
          customCss: require.resolve('./src/css/custom.css'),
        },
        sitemap: {
          changefreq: 'daily',
          priority: 0.8,
        },
      },
    ],
  ],
};
