import type { Config } from 'tailwindcss';

const config: Config = {
  content: ['./app/**/*.{ts,tsx}'],
  theme: {
    extend: {
      colors: {
        void: '#05030f',
        deep: '#0a0720',
        surface: '#120c2e',
        violet: '#7c5cff',
        'violet-bright': '#9d86ff',
        cyan: '#2ff0ff',
        gold: '#ffc857',
      },
      fontFamily: {
        display: ['var(--font-display)', 'Georgia', 'serif'],
        ui: ['var(--font-ui)', 'system-ui', 'sans-serif'],
        mono: ['var(--font-mono)', 'monospace'],
      },
    },
  },
  plugins: [],
};

export default config;
