import htmx from 'htmx.org';

window.htmx = htmx;

require('htmx.org/dist/ext/sse');
require('htmx.org/dist/ext/alpine-morph');

declare global {
  interface Window {
    htmx: typeof htmx;
  }
}
