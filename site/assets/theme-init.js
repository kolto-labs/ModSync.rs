// Restore both independent appearance choices before the page paints.
// With nothing saved, use the dark Kolto Labs theme.
(function () {
  var root = document.documentElement;
  try {
    var saved = localStorage.getItem('modsync-site-theme');
    if (saved === 'light' || saved === 'dark') root.dataset.theme = saved;
    var style = localStorage.getItem('modsync-site-style');
    if (style === 'kolto' || style === 'classic') root.dataset.siteStyle = style;
  } catch (e) {}
  var classic = root.dataset.siteStyle === 'classic';
  var light = root.dataset.theme === 'light';
  document.querySelector('meta[name="theme-color"]').content = classic
    ? (light ? '#f5f0e6' : '#14110d')
    : (light ? '#edf5f8' : '#03131c');
  if (classic) document.querySelector('link[rel="icon"]').href = 'assets/modsync-classic.svg';
})();
