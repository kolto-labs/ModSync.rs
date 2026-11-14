// Runs in <head> before the page paints, so a saved light or dark choice never flashes.
// With nothing saved, the page follows the system setting through CSS alone.
(function () {
  try {
    var saved = localStorage.getItem('modsync-site-theme');
    if (saved === 'light' || saved === 'dark') document.documentElement.dataset.theme = saved;
  } catch (e) {}
})();
