// Restore both independent appearance choices before the page paints.
// With nothing saved, use the dark Kolto Labs theme.
(function () {
  var root = document.documentElement;
  try {
    var saved = localStorage.getItem('modsync-site-theme');
    if (saved === 'light' || saved === 'dark') root.dataset.theme = saved;
    var style = localStorage.getItem('modsync-site-style');
    if (style === 'kolto' || style === 'classic' || style === 'modsync') root.dataset.siteStyle = style;
  } catch (e) {}
  var current = root.dataset.siteStyle || 'kolto';
  var light = root.dataset.theme === 'light';
  // [dark, light] browser chrome colour per site style (Kolto Labs, Classic, ModSync).
  var colors = { kolto: ['#03131c', '#edf5f8'], classic: ['#14110d', '#f5f0e6'], modsync: ['#000116', '#f5f5f5'] };
  document.querySelector('meta[name="theme-color"]').content = colors[current][light ? 1 : 0];
  if (current === 'classic') document.querySelector('link[rel="icon"]').href = 'assets/modsync-classic.svg';
})();
