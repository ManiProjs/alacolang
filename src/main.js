import './style.css';

const themeButton = document.querySelector('.theme-toggle');
const storedTheme = localStorage.getItem('alaco-theme');
const preferredTheme = window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
const applyTheme = (theme) => {
  document.documentElement.dataset.theme = theme;
  document.querySelector('meta[name="theme-color"]')?.setAttribute('content', theme === 'dark' ? '#171816' : '#ffffff');
  themeButton?.setAttribute('aria-label', `Switch to ${theme === 'dark' ? 'light' : 'dark'} mode`);
  if (themeButton) themeButton.innerHTML = `${theme === 'dark' ? '☀' : '◐'} <span>${theme === 'dark' ? 'Light' : 'Dark'}</span>`;
};
applyTheme(storedTheme || preferredTheme);
themeButton?.addEventListener('click', () => {
  const nextTheme = document.documentElement.dataset.theme === 'dark' ? 'light' : 'dark';
  localStorage.setItem('alaco-theme', nextTheme);
  applyTheme(nextTheme);
});

const menuButton = document.querySelector('.menu-toggle');
const nav = document.querySelector('.desktop-nav');
menuButton?.addEventListener('click', () => {
  const open = menuButton.getAttribute('aria-expanded') !== 'true';
  menuButton.setAttribute('aria-expanded', String(open));
  nav.classList.toggle('nav-open', open);
});
nav?.querySelectorAll('a').forEach((link) => {
  link.addEventListener('click', () => {
    nav.classList.remove('nav-open');
    menuButton?.setAttribute('aria-expanded', 'false');
  });
});

document.querySelector('.copy-command')?.addEventListener('click', async (event) => {
  const button = event.currentTarget;
  const commands = 'cargo install --path .\nalaco new hello\ncd hello\nalaco run\nalaco check';
  try {
    await navigator.clipboard.writeText(commands);
    button.textContent = 'Copied ✓';
  } catch {
    button.textContent = 'Select to copy';
  }
  window.setTimeout(() => { button.textContent = 'Copy commands'; }, 1800);
});

document.querySelector('#year').textContent = new Date().getFullYear();
