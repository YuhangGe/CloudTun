import { vm } from 'jinge';

export interface ThemeStore {
  isDark: boolean;
}

const themeMq = window.matchMedia('(prefers-color-scheme: dark)');
export const theme = vm({
  isDark: themeMq.matches,
});
function applyTheme() {
  document.body.classList[theme.isDark ? 'add' : 'remove']('dark');
}
applyTheme();

themeMq.addEventListener('change', (ev) => {
  theme.isDark = ev.matches;
});
