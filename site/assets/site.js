// Shared behaviour for every page of the site. Plain JavaScript, no build step.

// The GitHub repository this site points at (download, source, issues, clone commands).
// To move the site to another repository, change this one value. The pages also carry the
// same slug as a no-JavaScript fallback in their href attributes; this script rewrites
// every link marked with data-repo and every element marked with data-repo-url at load.
const REPO = 'kolto-labs/ModSync.rs';
const REPO_URL = 'https://github.com/' + REPO;

for (const link of document.querySelectorAll('a[data-repo]')) {
  link.href = REPO_URL + link.dataset.repo;
}
for (const node of document.querySelectorAll('[data-repo-url]')) {
  node.textContent = REPO_URL + (node.dataset.repoUrl || '');
}

// Appearance: follows the system until the reader picks light or dark, then remembers it.
const root = document.documentElement;
const darkQuery = matchMedia('(prefers-color-scheme: dark)');
const toggles = document.querySelectorAll('[data-theme-toggle]');
const current = () => root.dataset.theme || (darkQuery.matches ? 'dark' : 'light');
function syncToggles() {
  const dark = current() === 'dark';
  for (const button of toggles) {
    button.setAttribute('aria-pressed', String(dark));
    button.setAttribute('aria-label', dark ? 'Switch to light mode' : 'Switch to dark mode');
    button.title = dark ? 'Light mode' : 'Dark mode';
  }
}
for (const button of toggles) {
  button.addEventListener('click', () => {
    const next = current() === 'dark' ? 'light' : 'dark';
    root.dataset.theme = next;
    try {
      localStorage.setItem('modsync-site-theme', next);
    } catch (e) {}
    syncToggles();
  });
}
darkQuery.addEventListener('change', syncToggles);
syncToggles();
requestAnimationFrame(() => root.classList.add('theme-anim'));

// Docs: on a phone the sidebar is a header, and the menu button opens the page list.
const side = document.querySelector('.docs-side');
const menu = document.querySelector('#docs-menu');
function setMenu(open) {
  if (!side || !menu) return;
  side.toggleAttribute('data-open', open);
  menu.setAttribute('aria-expanded', String(open));
  menu.setAttribute('aria-label', open ? 'Hide the list of pages' : 'Show the list of pages');
}
menu?.addEventListener('click', () => setMenu(!side.hasAttribute('data-open')));
side?.querySelector('.docs-nav')?.addEventListener('click', (event) => {
  if (event.target.closest('a')) setMenu(false);
});
addEventListener('keydown', (event) => {
  if (event.key === 'Escape' && side?.hasAttribute('data-open')) {
    setMenu(false);
    menu.focus();
  }
});

// Docs: the outline marks the section in view.
const outline = [...document.querySelectorAll('.docs-outline a')];
if ('IntersectionObserver' in window && outline.length) {
  const mark = (id) => {
    for (const link of outline) {
      if (link.hash === '#' + id) link.setAttribute('aria-current', 'true');
      else link.removeAttribute('aria-current');
    }
  };
  const spy = new IntersectionObserver(
    (entries) => {
      for (const entry of entries) if (entry.isIntersecting) mark(entry.target.id);
    },
    { rootMargin: '-15% 0px -70% 0px' },
  );
  for (const link of outline) {
    const target = document.getElementById(link.hash.slice(1));
    if (target) spy.observe(target);
  }
}

// Code blocks: a copy button on each one.
for (const pre of document.querySelectorAll('pre.code')) {
  const button = document.createElement('button');
  button.type = 'button';
  button.className = 'copy';
  button.textContent = 'Copy';
  button.setAttribute('aria-label', 'Copy this code');
  button.addEventListener('click', async () => {
    const text = pre.querySelector('code')?.innerText ?? pre.innerText;
    try {
      await navigator.clipboard.writeText(text.replace(/\n$/, ''));
      button.textContent = 'Copied';
    } catch (e) {
      button.textContent = 'Press Ctrl+C';
      const range = document.createRange();
      range.selectNodeContents(pre.querySelector('code') ?? pre);
      const selection = getSelection();
      selection.removeAllRanges();
      selection.addRange(range);
    }
    setTimeout(() => (button.textContent = 'Copy'), 1800);
  });
  pre.append(button);
}
