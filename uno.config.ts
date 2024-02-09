import { defineConfig, presetUno } from 'unocss';

export default defineConfig({
  cli: {
    entry: {
      outFile: 'static/site.css',
      patterns: ['src/**/*.rs'],
    },
  },
  presets: [
    presetUno({ dark: 'media' }),
  ],
});
