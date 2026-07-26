import { defineConfig } from 'vitepress'
import tailwindcss from '@tailwindcss/vite'

export default defineConfig({
  title: 'claudex',
  description:
    'Query, search, and analyze Claude Code, OpenAI Codex, Pi, and OpenClaw sessions from the command line. A Rust CLI that indexes all four providers into one SQLite database and exposes reports as subcommands.',
  base: '/claudex/',

  vite: {
    plugins: [tailwindcss()],
    server: {
      allowedHosts: true,
    },
  },

  head: [
    [
      'link',
      { rel: 'icon', href: '/claudex/favicon.svg', type: 'image/svg+xml' },
    ],
    ['meta', { name: 'theme-color', content: '#D97757' }],
    ['meta', { property: 'og:type', content: 'website' }],
    ['meta', { property: 'og:title', content: 'claudex' }],
    [
      'meta',
      {
        property: 'og:description',
        content:
          'Query, search, and analyze Claude Code, OpenAI Codex, Pi, and OpenClaw sessions from the command line.',
      },
    ],
  ],

  lastUpdated: true,

  markdown: {
    theme: {
      light: 'catppuccin-latte',
      dark: 'catppuccin-mocha',
    },
  },

  sitemap: {
    hostname: 'https://utensils.github.io/claudex/',
  },

  themeConfig: {
    logo: '/favicon.svg',
    siteTitle: 'claudex',

    nav: [
      { text: 'Guide', link: '/guide/' },
      { text: 'Commands', link: '/commands/' },
      { text: 'Reference', link: '/reference/' },
      {
        text: 'v0.12.1', // x-release-please-version
        items: [
          {
            text: 'Changelog',
            link: 'https://github.com/utensils/claudex/releases',
          },
          {
            text: 'crates.io',
            link: 'https://crates.io/crates/claudex-cli',
          },
        ],
      },
    ],

    sidebar: {
      '/guide/': [
        {
          text: 'Getting Started',
          items: [
            { text: 'What is claudex?', link: '/guide/' },
            { text: 'Installation', link: '/guide/installation' },
            { text: 'Quickstart', link: '/guide/quickstart' },
            { text: 'How it works', link: '/guide/architecture' },
          ],
        },
        {
          text: 'Using claudex',
          items: [
            { text: 'Providers & filtering', link: '/guide/providers' },
            { text: 'The index', link: '/guide/indexing' },
            { text: 'JSON output', link: '/guide/json-output' },
            { text: 'Color & terminal', link: '/guide/color' },
            { text: 'Shell completions', link: '/guide/completions' },
            { text: 'Watch mode', link: '/guide/watch' },
          ],
        },
        {
          text: 'Integrations',
          items: [{ text: 'Claude Code Skill', link: '/guide/skill' }],
        },
        {
          text: 'Reference',
          items: [
            { text: 'Recipes', link: '/guide/recipes' },
            { text: 'Troubleshooting', link: '/guide/troubleshooting' },
          ],
        },
      ],
      '/commands/': [
        {
          text: 'Commands',
          items: [
            { text: 'Overview', link: '/commands/' },
            { text: 'summary', link: '/commands/summary' },
            { text: 'sessions', link: '/commands/sessions' },
            { text: 'session', link: '/commands/session' },
            { text: 'cost', link: '/commands/cost' },
            { text: 'search', link: '/commands/search' },
            { text: 'tools', link: '/commands/tools' },
            { text: 'models', link: '/commands/models' },
            { text: 'turns', link: '/commands/turns' },
            { text: 'prs', link: '/commands/prs' },
            { text: 'files', link: '/commands/files' },
            { text: 'providers', link: '/commands/providers' },
            { text: 'timeline', link: '/commands/timeline' },
            { text: 'budget', link: '/commands/budget' },
            { text: 'activity', link: '/commands/activity' },
            { text: 'export', link: '/commands/export' },
            { text: 'watch', link: '/commands/watch' },
            { text: 'index', link: '/commands/index-cmd' },
            { text: 'update', link: '/commands/update' },
            { text: 'completions', link: '/commands/completions' },
            { text: 'skills', link: '/commands/skills' },
          ],
        },
      ],
      '/reference/': [
        {
          text: 'Reference',
          items: [
            { text: 'Overview', link: '/reference/' },
            { text: 'File layout', link: '/reference/files' },
            { text: 'Index schema', link: '/reference/schema' },
            { text: 'Library API', link: '/reference/library' },
            { text: 'Pricing model', link: '/reference/pricing' },
            { text: 'Environment', link: '/reference/environment' },
          ],
        },
      ],
    },

    socialLinks: [
      { icon: 'github', link: 'https://github.com/utensils/claudex' },
    ],

    search: {
      provider: 'local',
    },

    footer: {
      message: 'Released under the MIT License.',
      copyright:
        'Copyright © <a href="https://jamesbrink.online/">James Brink</a>',
    },

    editLink: {
      pattern: 'https://github.com/utensils/claudex/edit/main/website/:path',
      text: 'Edit this page on GitHub',
    },

    outline: {
      level: [2, 3],
    },
  },
})
