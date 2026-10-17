import './style.css';

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
  const commands = 'cargo build\ncargo run -- new hello\ncd hello\ncargo run -- run\ncargo run -- check';
  try {
    await navigator.clipboard.writeText(commands);
    button.textContent = 'Copied ✓';
  } catch {
    button.textContent = 'Select to copy';
  }
  window.setTimeout(() => { button.textContent = 'Copy commands'; }, 1800);
});

document.querySelector('#year').textContent = new Date().getFullYear();
