// Mounts the Marketplace preview and keeps it in step with the site's appearance.
// ModSync style: dark lets the Marketplace's own K1 / TSL / Light switcher choose;
// light pins it to Light. Kolto Labs and Classic: the Marketplace borrows the site
// palette (marketplace.css), so only light vs dark matters for its rating colours.
(function () {
  var mount = document.getElementById('marketplace');
  var ds = window.ModSyncDesignSystem_2b35e3;
  if (!mount || !window.React || !window.ReactDOM || !ds || !ds.Marketplace) {
    if (mount) mount.innerHTML = '<div class="mk-fallback">The Marketplace preview could not load.</div>';
    return;
  }
  var root = document.documentElement;
  var reactRoot = window.ReactDOM.createRoot(mount);
  function render() {
    var style = root.dataset.siteStyle || 'kolto';
    var light = root.dataset.theme === 'light';
    var modsync = style === 'modsync';
    var props = {
      className: 'ms-scope',
      currentUser: 'You',
      showThemeSwitcher: modsync && !light,
      // Controlled unless the ModSync dark style hands the choice to the built-in switcher.
      theme: modsync && !light ? undefined : (light ? 'light' : 'k1'),
    };
    reactRoot.render(window.React.createElement(ds.Marketplace, props));
  }
  document.addEventListener('modsync:appearance', render);
  render();
})();
