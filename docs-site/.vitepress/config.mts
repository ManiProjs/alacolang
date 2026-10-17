import { defineConfig } from 'vitepress';

export default defineConfig({
  title: 'Alaco',
  description: 'Language guide, compiler reference, and standard library documentation for Alaco.',
  base: '/alaco/docs-site/',
  outDir: '../dist/docs-site',
  cleanUrls: true,
  head: [
    ['link', { rel: 'icon', type: 'image/svg+xml', href: '/alaco/docs-site/favicon.svg' }],
    ['link', { rel: 'preconnect', href: 'https://fonts.googleapis.com' }],
    ['link', { rel: 'preconnect', href: 'https://fonts.gstatic.com', crossorigin: '' }],
    ['link', { rel: 'stylesheet', href: 'https://fonts.googleapis.com/css2?family=DM+Sans:wght@400;500;600;700&family=Syne:wght@600;700;800&display=swap' }],
  ],
  themeConfig: {
    siteTitle: 'Alaco docs',
    nav: [
      { text: 'Guide', link: '/guide/getting-started' },
      { text: 'Language', link: '/guide/language' },
      { text: 'Reference', link: '/reference/cli' },
      { text: 'GitHub ↗', link: 'https://github.com/ManiProjs/alaco' },
    ],
    sidebar: {
      '/guide/': [
        { text: 'Start here', items: [
          { text: 'Introduction', link: '/' },
          { text: 'Getting started', link: '/guide/getting-started' },
          { text: 'Project workflow', link: '/guide/project-workflow' },
        ] },
        { text: 'Learn Alaco', items: [
          { text: 'Language guide', link: '/guide/language' },
          { text: 'Compiler targets', link: '/guide/targets' },
          { text: 'Editor support & LSP', link: '/guide/editor-support' },
        ] },
      ],
      '/reference/': [
        { text: 'Reference', items: [
          { text: 'CLI commands', link: '/reference/cli' },
          { text: 'Standard library', link: '/reference/standard-library' },
          { text: 'Project manifest', link: '/reference/manifest' },
          { text: 'Compiler architecture', link: '/reference/architecture' },
          { text: 'Current limitations', link: '/reference/limitations' },
        ] },
      ],
      '/tutorial/': [
        { text: 'Tutorials', items: [ { text: 'Build a small CLI', link: '/tutorial/first-cli' } ] },
      ],
    },
    search: { provider: 'local' },
    socialLinks: [{ icon: 'github', link: 'https://github.com/ManiProjs/alaco' }],
    footer: {
      message: 'Alaco is an evolving open-source language. Documentation describes the current implementation.',
      copyright: '© Alaco contributors',
    },
  },
});
