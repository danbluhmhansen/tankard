import htmx from 'htmx.org';
window.htmx = htmx;
declare global {
  interface Window {
    htmx: typeof htmx;
  }
}
