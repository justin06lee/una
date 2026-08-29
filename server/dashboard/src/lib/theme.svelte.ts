/**
 * Theme store: light / dark / follow-the-system.
 *
 * The initial `data-theme` attribute is set by an inline script in index.html
 * so the first paint is already correct; this module takes over afterwards and
 * keeps the attribute, localStorage, and the OS preference in sync.
 */

export type ThemeChoice = 'light' | 'dark' | 'system';

const STORAGE_KEY = 'una:theme';

function stored(): ThemeChoice {
  const raw = localStorage.getItem(STORAGE_KEY);
  return raw === 'light' || raw === 'dark' || raw === 'system' ? raw : 'system';
}

const media = window.matchMedia('(prefers-color-scheme: dark)');

class Theme {
  /** What the user picked. */
  choice = $state<ThemeChoice>(stored());
  /** What is actually on screen once `system` is resolved. */
  resolved = $state<'light' | 'dark'>('light');

  constructor() {
    this.apply();
    media.addEventListener('change', () => {
      if (this.choice === 'system') this.apply();
    });
    // Colour transitions are enabled only after the first paint, so loading the
    // page in dark mode doesn't fade in from light.
    requestAnimationFrame(() => document.documentElement.classList.add('theme-ready'));
  }

  private apply(): void {
    this.resolved = this.choice === 'system' ? (media.matches ? 'dark' : 'light') : this.choice;
    document.documentElement.dataset.theme = this.resolved;
  }

  set(choice: ThemeChoice): void {
    this.choice = choice;
    localStorage.setItem(STORAGE_KEY, choice);
    this.apply();
  }

  /** Cycle light → dark → system, for the sidebar control. */
  cycle(): void {
    this.set(this.choice === 'light' ? 'dark' : this.choice === 'dark' ? 'system' : 'light');
  }
}

export const theme = new Theme();
