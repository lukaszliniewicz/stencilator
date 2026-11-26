/**
 * Light, dark, or whatever the system prefers. The choice is remembered and set as
 * `data-theme` on the root element, which the colour tokens in app.css follow.
 */
import { readStored, writeStored } from './storage';

export type ThemeChoice = 'light' | 'dark' | 'auto';

export const THEMES: readonly ThemeChoice[] = ['light', 'dark', 'auto'];
const KEY = 'stencilator.theme';

function stored(): ThemeChoice {
  const value = readStored(KEY);
  return THEMES.find((theme) => theme === value) ?? 'auto';
}

function apply(choice: ThemeChoice): void {
  document.documentElement.dataset.theme = choice;
}

/** Sets the remembered theme before the interface first paints. */
export function applyStoredTheme(): void {
  apply(stored());
}

export class Theme {
  choice: ThemeChoice = $state(stored());
  #systemDark = $state(false);
  /** Whether the page is dark now, for drawing that cannot use the colour tokens. */
  readonly dark = $derived(this.choice === 'dark' || (this.choice === 'auto' && this.#systemDark));

  constructor() {
    const query = window.matchMedia('(prefers-color-scheme: dark)');
    this.#systemDark = query.matches;
    query.addEventListener('change', (event) => {
      this.#systemDark = event.matches;
    });
    $effect(() => {
      apply(this.choice);
      writeStored(KEY, this.choice);
    });
  }

  pick = (choice: ThemeChoice): void => {
    this.choice = choice;
  };
}
