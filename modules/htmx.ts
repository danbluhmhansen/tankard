import htmx from 'htmx.org';
window.htmx = htmx;
require('htmx.org/dist/ext/sse');
declare global {
  interface Window {
    htmx: typeof htmx;
  }
}
