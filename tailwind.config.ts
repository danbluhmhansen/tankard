import type { Config } from 'tailwindcss'

export default {
  content: ["./**/*.rs"],
  theme: {
    extend: {},
  },
  plugins: [
    require('@tailwindcss/forms'),    
  ],
} satisfies Config

