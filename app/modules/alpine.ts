import Alpine from 'alpinejs';
import morph from '@alpinejs/morph';

window.Alpine = Alpine;

Alpine.plugin(morph);
Alpine.start();

declare global {
  interface Window {
    Alpine: typeof Alpine;
  }
}
