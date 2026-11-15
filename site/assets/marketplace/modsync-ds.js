/* @ds-bundle: {"format":4,"namespace":"ModSyncDesignSystem_2b35e3","components":[{"name":"Badge"},{"name":"Button"},{"name":"Card"},{"name":"ProgressBar"},{"name":"Checkbox"},{"name":"Select"},{"name":"TextInput"},{"name":"ToggleSwitch"},{"name":"ModListItem"},{"name":"ThemeSwitcher"},{"name":"Tabs"},{"name":"StarRating"},{"name":"RatingSummary"},{"name":"ReviewCard"},{"name":"ReviewComposer"},{"name":"InstallButton"},{"name":"ModCard"},{"name":"Marketplace"}]} */

(() => {

const __ds_ns = (window.ModSyncDesignSystem_2b35e3 = window.ModSyncDesignSystem_2b35e3 || {});

const __ds_scope = {};

(__ds_ns.__errors = __ds_ns.__errors || []);

// components/core/Badge.jsx
try { (() => {
/**
 * ModSync Badge — the compact chips that label mods by category, tier
 * and validation status throughout the mod list and summary views.
 */

const CSS = `
.ms-badge{
  font-family:var(--font-default);
  display:inline-flex;align-items:center;gap:5px;
  padding:3px 9px;border-radius:var(--radius-sm);
  font-size:var(--text-2xs);font-weight:var(--weight-bold);
  letter-spacing:.03em;line-height:1.4;white-space:nowrap;
}
/* category = primary-tinted */
.ms-badge--category{background:var(--surface-raised);color:var(--brand-primary)}
/* tier = purple accent (its own scale per theme) */
.ms-badge--tier{background:var(--tier-fill);color:var(--tier-text)}
/* status variants share the semantic palette */
.ms-badge--success{background:rgba(76,175,80,.16);color:var(--success)}
.ms-badge--dependency{background:rgba(76,175,80,.16);color:var(--dependency)}
.ms-badge--restriction{background:rgba(244,67,54,.16);color:var(--restriction)}
.ms-badge--warning{background:rgba(255,165,0,.16);color:var(--warning)}
.ms-badge--error{background:rgba(255,107,107,.16);color:var(--error)}
.ms-badge--neutral{background:var(--surface-raised);color:var(--text-secondary)}
.ms-badge__dot{width:7px;height:7px;border-radius:50%;background:currentColor}
`;
function inject() {
  if (typeof document === "undefined") return;
  if (document.getElementById("ms-badge-css")) return;
  const s = document.createElement("style");
  s.id = "ms-badge-css";
  s.textContent = CSS;
  document.head.appendChild(s);
}
function Badge({
  variant = "neutral",
  dot = false,
  className = "",
  children,
  ...rest
}) {
  inject();
  const cls = `ms-badge ms-badge--${variant} ${className}`.trim();
  return React.createElement("span", {
    className: cls,
    ...rest
  }, dot ? React.createElement("span", {
    className: "ms-badge__dot",
    "aria-hidden": true
  }) : null, children);
}
Object.assign(__ds_scope, { Badge });
})(); } catch (e) { __ds_ns.__errors.push({ path: "components/core/Badge.jsx", error: String((e && e.message) || e) }); }

// components/core/Button.jsx
try { (() => {
/**
 * ModSync Button — the app's outlined-on-dark control, plus a filled
 * primary, a quiet ghost, and an inline link variant. Theme-aware via
 * the design-system CSS custom properties (K1 / TSL / Light).
 */

const CSS = `
.ms-btn{
  font-family:var(--font-default);
  display:inline-flex;align-items:center;justify-content:center;gap:8px;
  border-style:solid;border-width:var(--border-base);
  border-radius:var(--radius-sm);cursor:pointer;
  font-weight:var(--weight-normal);line-height:1;white-space:nowrap;
  transition:background var(--transition),border-color var(--transition),color var(--transition),opacity var(--transition);
  user-select:none;
}
.ms-btn:focus-visible{outline:none;box-shadow:var(--focus-ring)}
.ms-btn--sm{min-width:0;padding:6px 12px;font-size:var(--text-sm)}
.ms-btn--md{min-width:100px;padding:10px 16px;font-size:var(--text-md)}
.ms-btn--lg{min-width:120px;padding:13px 22px;font-size:var(--text-lg);border-radius:var(--radius-xl)}

/* secondary = the canonical KOTOR outlined button */
.ms-btn--secondary{background:var(--surface-card);border-color:var(--border-default);color:var(--text-primary)}
.ms-btn--secondary:hover{border-color:var(--brand-accent);color:var(--brand-accent)}
.ms-btn--secondary:active{border-color:var(--brand-primary);color:var(--brand-primary)}

/* primary = filled accent */
.ms-btn--primary{background:var(--brand-primary);border-color:var(--brand-primary);color:var(--text-on-accent);font-weight:var(--weight-semibold)}
.ms-btn--primary:hover{background:var(--brand-primary-bright);border-color:var(--brand-primary-bright)}
.ms-btn--primary:active{opacity:.9}

/* ghost = transparent until hover */
.ms-btn--ghost{background:transparent;border-color:transparent;color:var(--text-primary)}
.ms-btn--ghost:hover{background:var(--surface-raised);color:var(--brand-accent)}

/* link = inline text action */
.ms-btn--link{background:transparent;border-color:transparent;color:var(--brand-primary);min-width:0;padding:0;border-width:0}
.ms-btn--link:hover{color:var(--brand-primary-bright);text-decoration:underline}

.ms-btn:disabled{opacity:.5;cursor:not-allowed;pointer-events:none}
`;
function inject() {
  if (typeof document === "undefined") return;
  if (document.getElementById("ms-btn-css")) return;
  const s = document.createElement("style");
  s.id = "ms-btn-css";
  s.textContent = CSS;
  document.head.appendChild(s);
}
function Button({
  variant = "secondary",
  size = "md",
  icon = null,
  iconRight = null,
  disabled = false,
  type = "button",
  className = "",
  children,
  ...rest
}) {
  inject();
  const cls = `ms-btn ms-btn--${variant} ms-btn--${size} ${className}`.trim();
  return React.createElement("button", {
    type,
    className: cls,
    disabled,
    ...rest
  }, icon != null ? React.createElement("span", {
    className: "ms-btn__icon",
    "aria-hidden": true
  }, icon) : null, children != null ? React.createElement("span", null, children) : null, iconRight != null ? React.createElement("span", {
    className: "ms-btn__icon",
    "aria-hidden": true
  }, iconRight) : null);
}
Object.assign(__ds_scope, { Button });
})(); } catch (e) { __ds_ns.__errors.push({ path: "components/core/Button.jsx", error: String((e && e.message) || e) }); }

// components/core/Card.jsx
try { (() => {
/**
 * ModSync Card — bordered surface used for mod entries, summary panels
 * and the landing tiles. `interactive` adds the hover treatment used by
 * clickable list/download rows.
 */

const CSS = `
.ms-card{
  font-family:var(--font-default);
  background:var(--surface-card);
  border:var(--border-thin) solid var(--border-default);
  border-radius:var(--radius-md);
  color:var(--text-primary);
  padding:var(--space-6);
  display:block;
}
.ms-card--raised{background:var(--surface-raised)}
.ms-card--landing{border-radius:var(--radius-3xl);padding:var(--space-9);box-shadow:var(--shadow-card)}
.ms-card--interactive{transition:background var(--transition),border-color var(--transition);cursor:pointer}
.ms-card--interactive:hover{border-color:var(--brand-primary);background:var(--surface-hover)}
.ms-card__title{font-size:var(--text-lg);font-weight:var(--weight-semibold);color:var(--text-heading);margin:0 0 var(--space-4)}
`;
function inject() {
  if (typeof document === "undefined") return;
  if (document.getElementById("ms-card-css")) return;
  const s = document.createElement("style");
  s.id = "ms-card-css";
  s.textContent = CSS;
  document.head.appendChild(s);
}
function Card({
  variant = "default",
  interactive = false,
  title = null,
  className = "",
  children,
  ...rest
}) {
  inject();
  const cls = `ms-card ${variant !== "default" ? `ms-card--${variant}` : ""} ${interactive ? "ms-card--interactive" : ""} ${className}`.replace(/\s+/g, " ").trim();
  return React.createElement("div", {
    className: cls,
    ...rest
  }, title != null ? React.createElement("div", {
    className: "ms-card__title"
  }, title) : null, children);
}
Object.assign(__ds_scope, { Card });
})(); } catch (e) { __ds_ns.__errors.push({ path: "components/core/Card.jsx", error: String((e && e.message) || e) }); }

// components/feedback/ProgressBar.jsx
try { (() => {
/**
 * ModSync ProgressBar — install / download progress. Determinate track
 * in the theme primary, or an indeterminate "loading file…" sweep.
 */

const CSS = `
@keyframes ms-prog-sweep{0%{left:-35%}100%{left:100%}}
.ms-progress{font-family:var(--font-default);display:flex;flex-direction:column;gap:6px}
.ms-progress__head{display:flex;justify-content:space-between;align-items:baseline;font-size:var(--text-xs)}
.ms-progress__label{color:var(--text-secondary)}
.ms-progress__pct{color:var(--text-primary);font-variant-numeric:tabular-nums;font-family:var(--font-mono)}
.ms-progress__track{position:relative;height:8px;border-radius:var(--radius-pill);background:var(--surface-raised);overflow:hidden}
.ms-progress__fill{height:100%;border-radius:inherit;background:var(--brand-primary);transition:width 240ms ease}
.ms-progress--lg .ms-progress__track{height:18px;border-radius:var(--radius-xs)}
.ms-progress--success .ms-progress__fill{background:var(--success)}
.ms-progress--warning .ms-progress__fill{background:var(--warning)}
.ms-progress__indet{position:absolute;top:0;height:100%;width:35%;border-radius:inherit;background:var(--brand-primary);animation:ms-prog-sweep 1.1s ease-in-out infinite}
`;
function inject() {
  if (typeof document === "undefined") return;
  if (document.getElementById("ms-progress-css")) return;
  const s = document.createElement("style");
  s.id = "ms-progress-css";
  s.textContent = CSS;
  document.head.appendChild(s);
}
function ProgressBar({
  value = 0,
  max = 100,
  label = null,
  showValue = false,
  indeterminate = false,
  size = "md",
  status = "default",
  className = "",
  ...rest
}) {
  inject();
  const pct = Math.max(0, Math.min(100, value / max * 100));
  const cls = `ms-progress ms-progress--${size} ${status !== "default" ? `ms-progress--${status}` : ""} ${className}`.replace(/\s+/g, " ").trim();
  return React.createElement("div", {
    className: cls,
    ...rest
  }, label != null || showValue ? React.createElement("div", {
    className: "ms-progress__head"
  }, React.createElement("span", {
    className: "ms-progress__label"
  }, label), showValue ? React.createElement("span", {
    className: "ms-progress__pct"
  }, `${Math.round(pct)}%`) : null) : null, React.createElement("div", {
    className: "ms-progress__track",
    role: "progressbar",
    "aria-valuenow": indeterminate ? undefined : Math.round(pct),
    "aria-valuemin": 0,
    "aria-valuemax": 100
  }, indeterminate ? React.createElement("div", {
    className: "ms-progress__indet"
  }) : React.createElement("div", {
    className: "ms-progress__fill",
    style: {
      width: `${pct}%`
    }
  })));
}
Object.assign(__ds_scope, { ProgressBar });
})(); } catch (e) { __ds_ns.__errors.push({ path: "components/feedback/ProgressBar.jsx", error: String((e && e.message) || e) }); }

// components/forms/Checkbox.jsx
try { (() => {
/**
 * ModSync Checkbox — the mod-selection control. Square box that fills
 * with the theme primary when checked; supports an indeterminate state
 * for "some mods in this category selected".
 */

const CSS = `
.ms-check{font-family:var(--font-default);display:inline-flex;align-items:flex-start;gap:9px;cursor:pointer;color:var(--text-primary);font-size:var(--text-md);line-height:1.4}
.ms-check input{position:absolute;opacity:0;width:0;height:0}
.ms-check__box{
  flex:0 0 auto;width:18px;height:18px;margin-top:1px;
  background:var(--surface-card);
  border:var(--border-base) solid var(--border-default);
  border-radius:3px;
  display:inline-flex;align-items:center;justify-content:center;
  transition:border-color var(--transition),background var(--transition);
}
.ms-check:hover .ms-check__box{border-color:var(--brand-accent)}
.ms-check__box svg{width:12px;height:12px;stroke:var(--brand-primary);stroke-width:3;fill:none;opacity:0;transition:opacity var(--transition)}
.ms-check input:checked + .ms-check__box{border-color:var(--brand-primary)}
.ms-check input:checked + .ms-check__box svg{opacity:1}
.ms-check input:indeterminate + .ms-check__box::after{content:"";width:9px;height:2px;background:var(--brand-primary);border-radius:1px}
.ms-check input:focus-visible + .ms-check__box{box-shadow:var(--focus-ring)}
.ms-check--disabled{opacity:.5;cursor:not-allowed}
`;
function inject() {
  if (typeof document === "undefined") return;
  if (document.getElementById("ms-check-css")) return;
  const s = document.createElement("style");
  s.id = "ms-check-css";
  s.textContent = CSS;
  document.head.appendChild(s);
}
function Checkbox({
  checked,
  defaultChecked,
  indeterminate = false,
  disabled = false,
  onChange,
  label,
  className = "",
  ...rest
}) {
  inject();
  const ref = React.useRef(null);
  React.useEffect(() => {
    if (ref.current) ref.current.indeterminate = indeterminate;
  }, [indeterminate]);
  const cls = `ms-check ${disabled ? "ms-check--disabled" : ""} ${className}`.trim();
  return React.createElement("label", {
    className: cls
  }, React.createElement("input", {
    ref,
    type: "checkbox",
    checked,
    defaultChecked,
    disabled,
    onChange,
    ...rest
  }), React.createElement("span", {
    className: "ms-check__box",
    "aria-hidden": true
  }, React.createElement("svg", {
    viewBox: "0 0 14 14"
  }, React.createElement("polyline", {
    points: "2,7 6,11 12,3"
  }))), label != null ? React.createElement("span", null, label) : null);
}
Object.assign(__ds_scope, { Checkbox });
})(); } catch (e) { __ds_ns.__errors.push({ path: "components/forms/Checkbox.jsx", error: String((e && e.message) || e) }); }

// components/forms/Select.jsx
try { (() => {
/**
 * ModSync Select — a styled wrapper over a native <select>, matching the
 * app ComboBox (raised background, 2px border, accent on focus). Used for
 * tier pickers, install actions and category filters.
 */

const CSS = `
.ms-select{font-family:var(--font-default);display:flex;flex-direction:column;gap:6px}
.ms-select__label{font-size:var(--text-sm);font-weight:var(--weight-semibold);color:var(--text-secondary);letter-spacing:.02em}
.ms-select__box{position:relative;display:flex;align-items:center}
.ms-select select{
  appearance:none;-webkit-appearance:none;width:100%;
  background:var(--surface-raised);
  border:var(--border-base) solid var(--border-default);
  border-radius:var(--radius-sm);
  color:var(--text-primary);font-family:inherit;font-size:var(--text-md);
  padding:8px 34px 8px 11px;cursor:pointer;transition:border-color var(--transition);
}
.ms-select select:hover{border-color:var(--brand-accent)}
.ms-select select:focus{outline:none;border-color:var(--brand-primary)}
.ms-select__chevron{position:absolute;right:11px;pointer-events:none;color:var(--text-secondary);font-size:11px}
.ms-select--disabled{opacity:.5;pointer-events:none}
`;
function inject() {
  if (typeof document === "undefined") return;
  if (document.getElementById("ms-select-css")) return;
  const s = document.createElement("style");
  s.id = "ms-select-css";
  s.textContent = CSS;
  document.head.appendChild(s);
}
function Select({
  label,
  options = [],
  value,
  defaultValue,
  onChange,
  disabled = false,
  className = "",
  children,
  ...rest
}) {
  inject();
  const cls = `ms-select ${disabled ? "ms-select--disabled" : ""} ${className}`.trim();
  const opts = options.map(o => {
    const opt = typeof o === "string" ? {
      value: o,
      label: o
    } : o;
    return React.createElement("option", {
      key: opt.value,
      value: opt.value
    }, opt.label);
  });
  return React.createElement("label", {
    className: cls
  }, label != null ? React.createElement("span", {
    className: "ms-select__label"
  }, label) : null, React.createElement("span", {
    className: "ms-select__box"
  }, React.createElement("select", {
    value,
    defaultValue,
    onChange,
    disabled,
    ...rest
  }, children || opts), React.createElement("span", {
    className: "ms-select__chevron",
    "aria-hidden": true
  }, "▼")));
}
Object.assign(__ds_scope, { Select });
})(); } catch (e) { __ds_ns.__errors.push({ path: "components/forms/Select.jsx", error: String((e && e.message) || e) }); }

// components/forms/TextInput.jsx
try { (() => {
/**
 * ModSync TextInput — the app's TextBox: 2px border, primary text,
 * accent border on focus. Supports a label, a search affordance, an
 * error state, and a monospace mode for paths / regex.
 */

const CSS = `
.ms-field{font-family:var(--font-default);display:flex;flex-direction:column;gap:6px}
.ms-field__label{font-size:var(--text-sm);font-weight:var(--weight-semibold);color:var(--text-secondary);letter-spacing:.02em}
.ms-input{
  display:flex;align-items:center;gap:8px;
  background:var(--surface-card);
  border:var(--border-base) solid var(--border-default);
  border-radius:var(--radius-sm);
  padding:8px 11px;transition:border-color var(--transition);
}
.ms-input:hover{border-color:var(--brand-accent)}
.ms-input:focus-within{border-color:var(--brand-primary)}
.ms-input input{
  flex:1;min-width:0;background:transparent;border:0;outline:none;
  color:var(--text-primary);font-family:inherit;font-size:var(--text-md);
  caret-color:var(--caret);
}
.ms-input input::placeholder{color:var(--text-secondary);opacity:.85}
.ms-input--mono input{font-family:var(--font-mono);font-size:var(--text-sm)}
.ms-input__icon{color:var(--text-secondary);display:inline-flex;flex:0 0 auto}
.ms-input--error{border-color:var(--restriction)}
.ms-field--disabled{opacity:.5;pointer-events:none}
.ms-field__hint{font-size:var(--text-2xs);color:var(--text-secondary)}
.ms-field__hint--error{color:var(--restriction)}
`;
function inject() {
  if (typeof document === "undefined") return;
  if (document.getElementById("ms-input-css")) return;
  const s = document.createElement("style");
  s.id = "ms-input-css";
  s.textContent = CSS;
  document.head.appendChild(s);
}
function TextInput({
  label,
  icon = null,
  mono = false,
  error = false,
  hint = null,
  disabled = false,
  className = "",
  ...rest
}) {
  inject();
  const wrapCls = `ms-field ${disabled ? "ms-field--disabled" : ""} ${className}`.trim();
  const inputCls = `ms-input ${mono ? "ms-input--mono" : ""} ${error ? "ms-input--error" : ""}`.trim();
  return React.createElement("label", {
    className: wrapCls
  }, label != null ? React.createElement("span", {
    className: "ms-field__label"
  }, label) : null, React.createElement("span", {
    className: inputCls
  }, icon != null ? React.createElement("span", {
    className: "ms-input__icon",
    "aria-hidden": true
  }, icon) : null, React.createElement("input", {
    disabled,
    ...rest
  })), hint != null ? React.createElement("span", {
    className: `ms-field__hint ${error ? "ms-field__hint--error" : ""}`
  }, hint) : null);
}
Object.assign(__ds_scope, { TextInput });
})(); } catch (e) { __ds_ns.__errors.push({ path: "components/forms/TextInput.jsx", error: String((e && e.message) || e) }); }

// components/forms/ToggleSwitch.jsx
try { (() => {
/**
 * ModSync ToggleSwitch — the labelled mode switches in the title bar
 * (Spoiler-Free, Editor Mode, Wizard Mode). Pill track with a sliding
 * thumb in the theme primary color.
 */

const CSS = `
.ms-switch{font-family:var(--font-default);display:inline-flex;align-items:center;gap:10px;cursor:pointer;color:var(--text-primary);font-size:var(--text-md);user-select:none}
.ms-switch input{position:absolute;opacity:0;width:0;height:0}
.ms-switch__track{
  flex:0 0 auto;width:40px;height:21px;border-radius:var(--radius-pill);
  background:var(--surface-card);
  border:var(--border-base) solid var(--border-default);
  position:relative;transition:border-color var(--transition),background var(--transition);
}
.ms-switch__thumb{
  position:absolute;top:1px;left:1px;width:15px;height:15px;border-radius:50%;
  background:var(--text-secondary);transition:transform var(--transition),background var(--transition);
}
.ms-switch:hover .ms-switch__track{border-color:var(--brand-accent)}
.ms-switch input:checked + .ms-switch__track{border-color:var(--brand-primary)}
.ms-switch input:checked + .ms-switch__track .ms-switch__thumb{transform:translateX(19px);background:var(--brand-primary)}
.ms-switch input:focus-visible + .ms-switch__track{box-shadow:var(--focus-ring)}
.ms-switch--disabled{opacity:.5;cursor:not-allowed}
`;
function inject() {
  if (typeof document === "undefined") return;
  if (document.getElementById("ms-switch-css")) return;
  const s = document.createElement("style");
  s.id = "ms-switch-css";
  s.textContent = CSS;
  document.head.appendChild(s);
}
function ToggleSwitch({
  checked,
  defaultChecked,
  disabled = false,
  onChange,
  label,
  className = "",
  ...rest
}) {
  inject();
  const cls = `ms-switch ${disabled ? "ms-switch--disabled" : ""} ${className}`.trim();
  return React.createElement("label", {
    className: cls
  }, React.createElement("input", {
    type: "checkbox",
    role: "switch",
    checked,
    defaultChecked,
    disabled,
    onChange,
    ...rest
  }), React.createElement("span", {
    className: "ms-switch__track",
    "aria-hidden": true
  }, React.createElement("span", {
    className: "ms-switch__thumb"
  })), label != null ? React.createElement("span", null, label) : null);
}
Object.assign(__ds_scope, { ToggleSwitch });
})(); } catch (e) { __ds_ns.__errors.push({ path: "components/forms/ToggleSwitch.jsx", error: String((e && e.message) || e) }); }

// components/modsync/ModListItem.jsx
try { (() => {
/**
 * ModListItem — the heart of the ModSync sidebar. A selectable mod row
 * with a drag handle, checkbox, name + author, category / tier badges,
 * dependency & conflict indicators, and an optional validation state.
 */

const CSS = `
.ms-mod{
  font-family:var(--font-default);
  display:flex;align-items:flex-start;gap:10px;
  background:var(--surface-card);
  border:var(--border-thin) solid var(--border-default);
  border-left-width:3px;border-left-color:var(--border-default);
  border-radius:var(--radius-sm);
  padding:10px 12px;transition:background var(--transition),border-color var(--transition);
}
.ms-mod:hover{background:var(--surface-hover);border-color:var(--brand-accent)}
.ms-mod--selected{border-left-color:var(--brand-primary)}
.ms-mod--error{border-left-color:var(--error)}
.ms-mod--warning{border-left-color:var(--warning)}
.ms-mod__handle{color:var(--text-secondary);cursor:grab;font-size:13px;line-height:1.4;letter-spacing:-2px;user-select:none;margin-top:2px}
.ms-mod__body{flex:1;min-width:0;display:flex;flex-direction:column;gap:6px}
.ms-mod__top{display:flex;flex-direction:column;gap:2px}
.ms-mod__name{font-size:var(--text-md);font-weight:var(--weight-semibold);color:var(--text-primary);line-height:1.3}
.ms-mod__author{font-size:var(--text-2xs);color:var(--text-secondary)}
.ms-mod__badges{display:flex;flex-wrap:wrap;gap:6px}
.ms-mod__rels{display:flex;flex-wrap:wrap;gap:6px}
.ms-mod__check{margin-top:1px}
`;
function inject() {
  if (typeof document === "undefined") return;
  if (document.getElementById("ms-mod-css")) return;
  const s = document.createElement("style");
  s.id = "ms-mod-css";
  s.textContent = CSS;
  document.head.appendChild(s);
}
function ModListItem({
  name,
  author,
  category,
  tier,
  selected = false,
  onSelectChange,
  dependencies = [],
  conflicts = [],
  status = "default",
  draggable = true,
  className = "",
  ...rest
}) {
  inject();
  const cls = `ms-mod ${selected ? "ms-mod--selected" : ""} ${status !== "default" ? `ms-mod--${status}` : ""} ${className}`.replace(/\s+/g, " ").trim();
  return React.createElement("div", {
    className: cls,
    ...rest
  }, draggable ? React.createElement("span", {
    className: "ms-mod__handle",
    title: "Drag to reorder",
    "aria-hidden": true
  }, "⋮⋮") : null, React.createElement("span", {
    className: "ms-mod__check"
  }, React.createElement(__ds_scope.Checkbox, {
    checked: selected,
    onChange: onSelectChange ? e => onSelectChange(e.target.checked) : undefined
  })), React.createElement("div", {
    className: "ms-mod__body"
  }, React.createElement("div", {
    className: "ms-mod__top"
  }, React.createElement("div", {
    className: "ms-mod__name"
  }, name), author ? React.createElement("div", {
    className: "ms-mod__author"
  }, `by ${author}`) : null), category || tier ? React.createElement("div", {
    className: "ms-mod__badges"
  }, category ? React.createElement(__ds_scope.Badge, {
    variant: "category"
  }, category) : null, tier ? React.createElement(__ds_scope.Badge, {
    variant: "tier"
  }, tier) : null) : null, dependencies.length || conflicts.length ? React.createElement("div", {
    className: "ms-mod__rels"
  }, dependencies.map((d, i) => React.createElement(__ds_scope.Badge, {
    key: `d${i}`,
    variant: "dependency",
    dot: true
  }, `Requires ${d}`)), conflicts.map((c, i) => React.createElement(__ds_scope.Badge, {
    key: `c${i}`,
    variant: "restriction",
    dot: true
  }, `Conflicts ${c}`))) : null));
}
Object.assign(__ds_scope, { ModListItem });
})(); } catch (e) { __ds_ns.__errors.push({ path: "components/modsync/ModListItem.jsx", error: String((e && e.message) || e) }); }

// components/modsync/ThemeSwitcher.jsx
try { (() => {
/**
 * ThemeSwitcher — the title-bar segmented control that flips ModSync
 * between its three skins: Light (☀), K1 and TSL. Controlled; pair with
 * a `theme-light` / `theme-tsl` class on an ancestor to actually switch.
 */

const CSS = `
.ms-theme{font-family:var(--font-default);display:inline-flex;border:var(--border-thin) solid var(--border-default);border-radius:var(--radius-sm);overflow:hidden}
.ms-theme__seg{
  appearance:none;border:0;cursor:pointer;background:var(--surface-card);
  color:var(--text-secondary);font-family:inherit;font-weight:var(--weight-bold);
  min-width:36px;height:28px;padding:0 8px;font-size:var(--text-xs);
  display:inline-flex;align-items:center;justify-content:center;
  transition:background var(--transition),color var(--transition);
  border-right:var(--border-thin) solid var(--border-default);
}
.ms-theme__seg:last-child{border-right:0}
.ms-theme__seg:hover{color:var(--brand-accent)}
.ms-theme__seg--active{background:var(--brand-primary);color:var(--text-on-accent)}
.ms-theme__seg--active:hover{color:var(--text-on-accent)}
`;
const SEGMENTS = [{
  id: "light",
  label: "☀",
  title: "Light Theme"
}, {
  id: "k1",
  label: "K1",
  title: "KOTOR 1 Theme"
}, {
  id: "tsl",
  label: "TSL",
  title: "KOTOR 2: TSL Theme"
}];
function inject() {
  if (typeof document === "undefined") return;
  if (document.getElementById("ms-theme-css")) return;
  const s = document.createElement("style");
  s.id = "ms-theme-css";
  s.textContent = CSS;
  document.head.appendChild(s);
}
function ThemeSwitcher({
  value = "k1",
  onChange,
  className = "",
  ...rest
}) {
  inject();
  const cls = `ms-theme ${className}`.trim();
  return React.createElement("div", {
    className: cls,
    role: "group",
    "aria-label": "Theme",
    ...rest
  }, SEGMENTS.map(seg => React.createElement("button", {
    key: seg.id,
    type: "button",
    title: seg.title,
    "aria-pressed": seg.id === value,
    className: `ms-theme__seg ${seg.id === value ? "ms-theme__seg--active" : ""}`.trim(),
    onClick: onChange ? () => onChange(seg.id) : undefined
  }, seg.label)));
}
Object.assign(__ds_scope, { ThemeSwitcher });
})(); } catch (e) { __ds_ns.__errors.push({ path: "components/modsync/ThemeSwitcher.jsx", error: String((e && e.message) || e) }); }

// components/navigation/Tabs.jsx
try { (() => {
/**
 * ModSync Tabs — the editor's tab strip (Getting Started / Summary /
 * Editor / Raw). Underline-style selection in the theme primary.
 */

const CSS = `
.ms-tabs{font-family:var(--font-default);display:flex;gap:2px;border-bottom:var(--border-thin) solid var(--border-default)}
.ms-tab{
  appearance:none;background:transparent;border:0;cursor:pointer;
  color:var(--text-secondary);font-family:inherit;font-size:var(--text-md);
  padding:10px 16px;position:relative;transition:color var(--transition);
  border-bottom:var(--border-base) solid transparent;margin-bottom:-1px;
  display:inline-flex;align-items:center;gap:7px;
}
.ms-tab:hover{color:var(--brand-accent)}
.ms-tab--active{color:var(--brand-primary);border-bottom-color:var(--brand-primary)}
.ms-tab:focus-visible{outline:none;color:var(--brand-primary)}
.ms-tab:disabled{opacity:.4;cursor:not-allowed}
.ms-tab__badge{font-size:var(--text-2xs);font-weight:var(--weight-bold);background:var(--surface-raised);color:inherit;border-radius:var(--radius-pill);padding:1px 7px}
`;
function inject() {
  if (typeof document === "undefined") return;
  if (document.getElementById("ms-tabs-css")) return;
  const s = document.createElement("style");
  s.id = "ms-tabs-css";
  s.textContent = CSS;
  document.head.appendChild(s);
}
function Tabs({
  tabs = [],
  value,
  onChange,
  className = "",
  ...rest
}) {
  inject();
  const cls = `ms-tabs ${className}`.trim();
  return React.createElement("div", {
    className: cls,
    role: "tablist",
    ...rest
  }, tabs.map(t => {
    const tab = typeof t === "string" ? {
      id: t,
      label: t
    } : t;
    const active = tab.id === value;
    return React.createElement("button", {
      key: tab.id,
      type: "button",
      role: "tab",
      "aria-selected": active,
      disabled: tab.disabled,
      className: `ms-tab ${active ? "ms-tab--active" : ""}`.trim(),
      onClick: onChange ? () => onChange(tab.id) : undefined
    }, tab.icon != null ? React.createElement("span", {
      "aria-hidden": true
    }, tab.icon) : null, tab.label, tab.badge != null ? React.createElement("span", {
      className: "ms-tab__badge"
    }, tab.badge) : null);
  }));
}
Object.assign(__ds_scope, { Tabs });
})(); } catch (e) { __ds_ns.__errors.push({ path: "components/navigation/Tabs.jsx", error: String((e && e.message) || e) }); }

// ui_kits/modsync-app/App.jsx
try { (() => {
// ModSync app shell — orchestrates landing → workspace → install
function App() {
  const [theme, setTheme] = React.useState("k1");
  const [editorMode, setEditorMode] = React.useState(false);
  const [spoilerFree, setSpoiler] = React.useState(false);
  const [screen, setScreen] = React.useState("landing");
  const [mods, setMods] = React.useState(() => window.MODSYNC_DATA.mods.map(m => ({
    ...m
  })));
  const [search, setSearch] = React.useState("");
  const [focusId, setFocusId] = React.useState("k1cp");
  const themeClass = theme === "k1" ? "" : "theme-" + theme;
  const toggleMod = id => setMods(ms => ms.map(m => m.id === id ? {
    ...m,
    selected: !m.selected
  } : m));
  const selectAll = () => setMods(ms => ms.map(m => ({
    ...m,
    selected: true
  })));
  const deselectAll = () => setMods(ms => ms.map(m => ({
    ...m,
    selected: false
  })));
  return /*#__PURE__*/React.createElement("div", {
    className: "ms-window " + themeClass
  }, /*#__PURE__*/React.createElement(TitleBar, {
    theme: theme,
    onTheme: setTheme,
    editorMode: editorMode,
    onEditorMode: v => {
      setEditorMode(v);
      if (v) setScreen("workspace");
    },
    spoilerFree: spoilerFree,
    onSpoiler: setSpoiler
  }), screen !== "landing" && /*#__PURE__*/React.createElement("div", {
    className: "ms-homerow"
  }, /*#__PURE__*/React.createElement("button", {
    type: "button",
    className: "ms-home",
    onClick: () => setScreen("landing")
  }, "\uD83C\uDFE0 Home")), /*#__PURE__*/React.createElement("div", {
    className: "ms-content"
  }, screen === "landing" && /*#__PURE__*/React.createElement(LandingScreen, {
    onLoad: () => setScreen("workspace"),
    onEditor: () => {
      setEditorMode(true);
      setScreen("workspace");
    }
  }), screen === "workspace" && /*#__PURE__*/React.createElement(Workspace, {
    mods: mods,
    search: search,
    onSearch: setSearch,
    focusId: focusId,
    onFocus: setFocusId,
    onToggle: toggleMod,
    onSelectAll: selectAll,
    onDeselectAll: deselectAll,
    onInstall: () => setScreen("install")
  }), screen === "install" && /*#__PURE__*/React.createElement(InstallProgress, {
    mods: mods,
    onDone: () => setScreen("landing")
  })));
}
window.App = App;
})(); } catch (e) { __ds_ns.__errors.push({ path: "ui_kits/modsync-app/App.jsx", error: String((e && e.message) || e) }); }

// ui_kits/modsync-app/InstallProgress.jsx
try { (() => {
// ModSync install progress — guided install with per-mod + overall bars
const {
  ProgressBar,
  Button,
  Badge
} = window.ModSyncDesignSystem_2b35e3;
function InstallProgress({
  mods,
  onDone
}) {
  const queue = mods.filter(m => m.selected);
  const [step, setStep] = React.useState(0);
  const [pct, setPct] = React.useState(0);
  const done = step >= queue.length;
  React.useEffect(() => {
    if (done) return;
    const t = setInterval(() => {
      setPct(p => {
        if (p >= 100) {
          setStep(s => s + 1);
          return 0;
        }
        return p + 8;
      });
    }, 120);
    return () => clearInterval(t);
  }, [step, done]);
  const overall = Math.min(100, Math.round((step + pct / 100) / queue.length * 100));
  return /*#__PURE__*/React.createElement("div", {
    className: "ms-scroll"
  }, /*#__PURE__*/React.createElement("div", {
    className: "ms-install"
  }, /*#__PURE__*/React.createElement("div", {
    className: "ms-install__head"
  }, /*#__PURE__*/React.createElement("div", {
    className: "ms-install__title"
  }, done ? "Installation complete" : "Installing your mod build"), /*#__PURE__*/React.createElement("div", {
    className: "ms-install__sub"
  }, done ? "All selected mods were installed inside the sandboxed game directory." : "Every file operation is sandboxed to your mod + game directories.")), /*#__PURE__*/React.createElement(ProgressBar, {
    value: overall,
    size: "lg",
    status: done ? "success" : "default",
    label: `Overall — ${done ? queue.length : step} of ${queue.length} mods`,
    showValue: true
  }), /*#__PURE__*/React.createElement("div", {
    className: "ms-install__list"
  }, queue.map((m, i) => {
    const state = i < step || done ? "done" : i === step ? "active" : "pending";
    return /*#__PURE__*/React.createElement("div", {
      className: "ms-install__row ms-install__row--" + state,
      key: m.id
    }, /*#__PURE__*/React.createElement("div", {
      className: "ms-install__row-top"
    }, /*#__PURE__*/React.createElement("span", {
      className: "ms-install__name"
    }, m.name), state === "done" && /*#__PURE__*/React.createElement(Badge, {
      variant: "success",
      dot: true
    }, "Installed"), state === "active" && /*#__PURE__*/React.createElement(Badge, {
      variant: "category"
    }, "Installing\u2026"), state === "pending" && /*#__PURE__*/React.createElement(Badge, {
      variant: "neutral"
    }, "Queued")), state === "active" && /*#__PURE__*/React.createElement(ProgressBar, {
      value: pct,
      label: m.instructions[0].action + " " + m.instructions[0].source,
      showValue: true
    }));
  })), done && /*#__PURE__*/React.createElement("div", {
    className: "ms-install__actions"
  }, /*#__PURE__*/React.createElement(Button, {
    variant: "primary",
    size: "lg",
    onClick: onDone,
    icon: "\uD83C\uDFE0"
  }, "Back to Home"), /*#__PURE__*/React.createElement(Button, {
    variant: "secondary",
    size: "lg"
  }, "Open Output Log"))));
}
window.InstallProgress = InstallProgress;
})(); } catch (e) { __ds_ns.__errors.push({ path: "ui_kits/modsync-app/InstallProgress.jsx", error: String((e && e.message) || e) }); }

// ui_kits/modsync-app/LandingScreen.jsx
try { (() => {
// ModSync landing / onboarding screen — mirrors LandingPageView.axaml
const {
  Button
} = window.ModSyncDesignSystem_2b35e3;
function LandingScreen({
  onLoad,
  onEditor
}) {
  const cards = [{
    icon: "📂",
    title: "Load a curated instruction set",
    body: "Import a ready-made configuration (.toml, .json, .yaml, .md) and let the wizard handle validation, downloads, and install order.",
    cta: /*#__PURE__*/React.createElement(Button, {
      variant: "secondary",
      icon: "\uD83D\uDD0E",
      onClick: onLoad
    }, "Browse Instruction File"),
    status: "No instruction file loaded yet."
  }, {
    icon: "🛠️",
    title: "Build your own instruction set",
    body: "Switch into the editor for full control over mods, instructions, tiers, and dependency rules with real-time validation.",
    cta: /*#__PURE__*/React.createElement(Button, {
      variant: "secondary",
      icon: "\u270F\uFE0F",
      onClick: onEditor
    }, "Open the Editor"),
    status: "Editor mode is currently off."
  }, {
    icon: "📘",
    title: "Follow the quick-start checklist",
    body: "Review the installation playbook, sandbox overview, and validation tips before your next mod session.",
    list: ["Secure sandboxed file operations", "Built-in wizard for guided installs", "Editor with raw + visual modes"]
  }, {
    icon: "💖",
    title: "Support development",
    body: "If you enjoy using ModSync, please consider supporting its development!",
    cta: /*#__PURE__*/React.createElement(Button, {
      variant: "secondary",
      icon: "\u2764\uFE0F"
    }, "Sponsor on GitHub"),
    status: "Your support helps maintain and improve this tool."
  }];
  const steps = [["1. Point to your KOTOR directories", "Use the directory selectors to set your dedicated mod workspace and the KOTOR installation you want to patch."], ["2. Load or author instructions", "Bring in a curated file or flip on the editor to craft instructions with dependency tracking, tiers, and validation hints."], ["3. Validate and download everything", "The wizard resolves downloads, checks archives in the sandbox, and highlights any issues before install time."], ["4. Install with confidence", "Launch the guided installer knowing every file stays inside the sandboxed mod + game directories."]];
  return /*#__PURE__*/React.createElement("div", {
    className: "ms-scroll"
  }, /*#__PURE__*/React.createElement("div", {
    className: "ms-landing"
  }, /*#__PURE__*/React.createElement("div", {
    className: "ms-hero"
  }, /*#__PURE__*/React.createElement("div", {
    className: "ms-hero__title"
  }, "Welcome to ModSync"), /*#__PURE__*/React.createElement("div", {
    className: "ms-hero__subtitle"
  }, "Launch into a curated mod build or craft your own galactic experience."), /*#__PURE__*/React.createElement("div", {
    className: "ms-hero__detail"
  }, "ModSync guides you through smart, secure installs with sandboxed file operations, wizard-style workflows, and a powerful editor for authors.")), /*#__PURE__*/React.createElement("div", {
    className: "ms-landing__cards"
  }, cards.map((c, i) => /*#__PURE__*/React.createElement("div", {
    className: "ms-lcard",
    key: i
  }, /*#__PURE__*/React.createElement("div", {
    className: "ms-lcard__head"
  }, /*#__PURE__*/React.createElement("span", {
    className: "ms-lcard__icon"
  }, c.icon), /*#__PURE__*/React.createElement("span", {
    className: "ms-lcard__title"
  }, c.title)), /*#__PURE__*/React.createElement("div", {
    className: "ms-lcard__body"
  }, c.body), c.cta, c.list ? /*#__PURE__*/React.createElement("div", {
    className: "ms-lcard__info"
  }, c.list.map((l, j) => /*#__PURE__*/React.createElement("div", {
    key: j
  }, "\u2022 ", l))) : /*#__PURE__*/React.createElement("div", {
    className: "ms-lcard__info ms-lcard__status"
  }, c.status)))), /*#__PURE__*/React.createElement("div", {
    className: "ms-section"
  }, /*#__PURE__*/React.createElement("div", {
    className: "ms-section__title"
  }, "Getting started in four quick steps"), /*#__PURE__*/React.createElement("div", {
    className: "ms-section__subtitle"
  }, "Follow this guided path to move from a fresh install to a fully modded galaxy in minutes."), steps.map(([t, b], i) => /*#__PURE__*/React.createElement("div", {
    className: "ms-step",
    key: i
  }, /*#__PURE__*/React.createElement("div", {
    className: "ms-step__title"
  }, t), /*#__PURE__*/React.createElement("div", {
    className: "ms-step__body"
  }, b))))));
}
window.LandingScreen = LandingScreen;
})(); } catch (e) { __ds_ns.__errors.push({ path: "ui_kits/modsync-app/LandingScreen.jsx", error: String((e && e.message) || e) }); }

// ui_kits/modsync-app/TitleBar.jsx
try { (() => {
// ModSync title bar — custom window chrome (NoChrome / BorderOnly in app)
const {
  ThemeSwitcher,
  ToggleSwitch
} = window.ModSyncDesignSystem_2b35e3;
function TitleBar({
  theme,
  onTheme,
  editorMode,
  onEditorMode,
  spoilerFree,
  onSpoiler
}) {
  return /*#__PURE__*/React.createElement("div", {
    className: "ms-titlebar"
  }, /*#__PURE__*/React.createElement("nav", {
    className: "ms-menu"
  }, ["File", "Edit", "Tools", "Help"].map(m => /*#__PURE__*/React.createElement("button", {
    key: m,
    className: "ms-menu__item",
    type: "button"
  }, m))), /*#__PURE__*/React.createElement(ToggleSwitch, {
    label: "Spoiler-Free",
    checked: spoilerFree,
    onChange: e => onSpoiler(e.target.checked)
  }), /*#__PURE__*/React.createElement("div", {
    className: "ms-titlebar__title"
  }, /*#__PURE__*/React.createElement("img", {
    className: "ms-titlebar__mark",
    src: theme === "tsl" ? "../../assets/modsync-mark-tsl.png" : "../../assets/modsync-mark-k1.png",
    alt: ""
  }), "ModSync"), /*#__PURE__*/React.createElement(ToggleSwitch, {
    label: "Editor Mode",
    checked: editorMode,
    onChange: e => onEditorMode(e.target.checked)
  }), /*#__PURE__*/React.createElement(ThemeSwitcher, {
    value: theme,
    onChange: onTheme
  }), /*#__PURE__*/React.createElement("div", {
    className: "ms-winctl"
  }, /*#__PURE__*/React.createElement("button", {
    type: "button",
    title: "Minimize"
  }, "\u2212"), /*#__PURE__*/React.createElement("button", {
    type: "button",
    title: "Maximize"
  }, "\u25A2"), /*#__PURE__*/React.createElement("button", {
    type: "button",
    className: "ms-winctl__close",
    title: "Close"
  }, "\u2715")));
}
window.TitleBar = TitleBar;
})(); } catch (e) { __ds_ns.__errors.push({ path: "ui_kits/modsync-app/TitleBar.jsx", error: String((e && e.message) || e) }); }

// ui_kits/modsync-app/TitleBar.standalone.jsx
try { (() => {
// ModSync title bar — standalone-build variant: logo marks come from inlined resources
const {
  ThemeSwitcher,
  ToggleSwitch
} = window.ModSyncDesignSystem_2b35e3;
function TitleBar({
  theme,
  onTheme,
  editorMode,
  onEditorMode,
  spoilerFree,
  onSpoiler
}) {
  return /*#__PURE__*/React.createElement("div", {
    className: "ms-titlebar"
  }, /*#__PURE__*/React.createElement("nav", {
    className: "ms-menu"
  }, ["File", "Edit", "Tools", "Help"].map(m => /*#__PURE__*/React.createElement("button", {
    key: m,
    className: "ms-menu__item",
    type: "button"
  }, m))), /*#__PURE__*/React.createElement(ToggleSwitch, {
    label: "Spoiler-Free",
    checked: spoilerFree,
    onChange: e => onSpoiler(e.target.checked)
  }), /*#__PURE__*/React.createElement("div", {
    className: "ms-titlebar__title"
  }, /*#__PURE__*/React.createElement("img", {
    className: "ms-titlebar__mark",
    src: theme === "tsl" ? window.__resources.markTsl : window.__resources.markK1,
    alt: ""
  }), "ModSync"), /*#__PURE__*/React.createElement(ToggleSwitch, {
    label: "Editor Mode",
    checked: editorMode,
    onChange: e => onEditorMode(e.target.checked)
  }), /*#__PURE__*/React.createElement(ThemeSwitcher, {
    value: theme,
    onChange: onTheme
  }), /*#__PURE__*/React.createElement("div", {
    className: "ms-winctl"
  }, /*#__PURE__*/React.createElement("button", {
    type: "button",
    title: "Minimize"
  }, "\u2212"), /*#__PURE__*/React.createElement("button", {
    type: "button",
    title: "Maximize"
  }, "\u25A2"), /*#__PURE__*/React.createElement("button", {
    type: "button",
    className: "ms-winctl__close",
    title: "Close"
  }, "\u2715")));
}
window.TitleBar = TitleBar;
})(); } catch (e) { __ds_ns.__errors.push({ path: "ui_kits/modsync-app/TitleBar.standalone.jsx", error: String((e && e.message) || e) }); }

// ui_kits/modsync-app/Workspace.jsx
try { (() => {
// ModSync main workspace — mod list sidebar + Summary/Editor tabs
const {
  Tabs,
  ModListItem,
  Badge,
  Button,
  TextInput
} = window.ModSyncDesignSystem_2b35e3;
function ModSidebar({
  mods,
  search,
  onSearch,
  focusId,
  onFocus,
  onToggle,
  onSelectAll,
  onDeselectAll
}) {
  const q = search.trim().toLowerCase();
  const shown = q ? mods.filter(m => m.name.toLowerCase().includes(q) || m.author.toLowerCase().includes(q)) : mods;
  const count = mods.filter(m => m.selected).length;
  return /*#__PURE__*/React.createElement("aside", {
    className: "ms-sidebar"
  }, /*#__PURE__*/React.createElement("div", {
    className: "ms-sidebar__head"
  }, /*#__PURE__*/React.createElement("div", {
    className: "ms-sidebar__title"
  }, "Mods ", /*#__PURE__*/React.createElement("span", {
    className: "ms-sidebar__count"
  }, count, "/", mods.length, " selected")), /*#__PURE__*/React.createElement(TextInput, {
    icon: "\uD83D\uDD0E",
    placeholder: "Search mods\u2026",
    value: search,
    onChange: e => onSearch(e.target.value)
  }), /*#__PURE__*/React.createElement("div", {
    className: "ms-sidebar__tools"
  }, /*#__PURE__*/React.createElement(Button, {
    variant: "ghost",
    size: "sm",
    onClick: onSelectAll
  }, "Select All"), /*#__PURE__*/React.createElement(Button, {
    variant: "ghost",
    size: "sm",
    onClick: onDeselectAll
  }, "Deselect All"), /*#__PURE__*/React.createElement(Button, {
    variant: "ghost",
    size: "sm"
  }, "By Tier \u25BE"))), /*#__PURE__*/React.createElement("div", {
    className: "ms-sidebar__list"
  }, shown.map(m => /*#__PURE__*/React.createElement("div", {
    key: m.id,
    onClick: () => onFocus(m.id),
    className: focusId === m.id ? "ms-sidebar__row ms-sidebar__row--focus" : "ms-sidebar__row"
  }, /*#__PURE__*/React.createElement(ModListItem, {
    name: m.name,
    author: m.author,
    category: m.category,
    tier: m.tier,
    selected: m.selected,
    onSelectChange: () => onToggle(m.id),
    dependencies: m.dependencies,
    conflicts: m.conflicts,
    status: m.status
  })))));
}
function SummaryPanel({
  mod
}) {
  if (!mod) return /*#__PURE__*/React.createElement("div", {
    className: "ms-detail__empty"
  }, "Select a mod to see its summary.");
  const colors = window.MODSYNC_DATA.actionColor;
  return /*#__PURE__*/React.createElement("div", {
    className: "ms-scroll"
  }, /*#__PURE__*/React.createElement("div", {
    className: "ms-detail"
  }, /*#__PURE__*/React.createElement("div", {
    className: "ms-detail__header"
  }, /*#__PURE__*/React.createElement("div", null, /*#__PURE__*/React.createElement("div", {
    className: "ms-detail__name"
  }, mod.name), /*#__PURE__*/React.createElement("div", {
    className: "ms-detail__author"
  }, "by ", mod.author)), /*#__PURE__*/React.createElement("div", {
    className: "ms-detail__badges"
  }, /*#__PURE__*/React.createElement(Badge, {
    variant: "category"
  }, mod.category), /*#__PURE__*/React.createElement(Badge, {
    variant: "tier"
  }, mod.tier))), /*#__PURE__*/React.createElement("p", {
    className: "ms-detail__desc"
  }, mod.description), (mod.dependencies || mod.conflicts) && /*#__PURE__*/React.createElement("div", {
    className: "ms-detail__rels"
  }, (mod.dependencies || []).map((d, i) => /*#__PURE__*/React.createElement(Badge, {
    key: "d" + i,
    variant: "dependency",
    dot: true
  }, "Requires ", d)), (mod.conflicts || []).map((c, i) => /*#__PURE__*/React.createElement(Badge, {
    key: "c" + i,
    variant: "restriction",
    dot: true
  }, "Conflicts with ", c))), /*#__PURE__*/React.createElement("div", {
    className: "ms-detail__label"
  }, "Mod Links"), /*#__PURE__*/React.createElement("div", {
    className: "ms-detail__links"
  }, mod.links.map((l, i) => /*#__PURE__*/React.createElement("a", {
    key: i,
    className: "ms-link",
    href: "https://" + l,
    target: "_blank",
    rel: "noreferrer"
  }, "\uD83D\uDD17 ", l))), mod.options && /*#__PURE__*/React.createElement(React.Fragment, null, /*#__PURE__*/React.createElement("div", {
    className: "ms-detail__label"
  }, "Options"), /*#__PURE__*/React.createElement("div", {
    className: "ms-detail__options"
  }, mod.options.map((o, i) => /*#__PURE__*/React.createElement("label", {
    key: i,
    className: "ms-opt"
  }, /*#__PURE__*/React.createElement("input", {
    type: "radio",
    name: "opt",
    defaultChecked: i === mod.options.length - 1
  }), " ", o)))), /*#__PURE__*/React.createElement("div", {
    className: "ms-detail__label"
  }, "Instructions"), /*#__PURE__*/React.createElement("div", {
    className: "ms-detail__instr"
  }, mod.instructions.map((ins, i) => /*#__PURE__*/React.createElement("div", {
    className: "ms-instr",
    key: i
  }, /*#__PURE__*/React.createElement("span", {
    className: "ms-instr__action",
    style: {
      color: colors[ins.action]
    }
  }, ins.action), /*#__PURE__*/React.createElement("code", {
    className: "ms-instr__src"
  }, ins.source), ins.dest && /*#__PURE__*/React.createElement("span", {
    className: "ms-instr__arrow"
  }, "\u2192"), ins.dest && /*#__PURE__*/React.createElement("code", {
    className: "ms-instr__dest"
  }, ins.dest), ins.args && /*#__PURE__*/React.createElement("span", {
    className: "ms-instr__args"
  }, "args: ", ins.args))))));
}
function Workspace({
  mods,
  search,
  onSearch,
  focusId,
  onFocus,
  onToggle,
  onSelectAll,
  onDeselectAll,
  onInstall
}) {
  const [tab, setTab] = React.useState("summary");
  const focused = mods.find(m => m.id === focusId);
  const selectedCount = mods.filter(m => m.selected).length;
  const conflictPairs = mods.filter(m => m.selected && m.conflicts && mods.some(o => o.selected && m.conflicts.includes(o.name))).length;
  return /*#__PURE__*/React.createElement("div", {
    className: "ms-workspace"
  }, /*#__PURE__*/React.createElement(ModSidebar, {
    mods,
    search,
    onSearch,
    focusId,
    onFocus,
    onToggle,
    onSelectAll,
    onDeselectAll
  }), /*#__PURE__*/React.createElement("div", {
    className: "ms-main"
  }, /*#__PURE__*/React.createElement(Tabs, {
    value: tab,
    onChange: setTab,
    tabs: [{
      id: "summary",
      label: "Summary"
    }, {
      id: "editor",
      label: "Editor"
    }, {
      id: "raw",
      label: "Raw"
    }]
  }), /*#__PURE__*/React.createElement("div", {
    className: "ms-main__body"
  }, tab === "summary" && /*#__PURE__*/React.createElement(SummaryPanel, {
    mod: focused
  }), tab === "editor" && /*#__PURE__*/React.createElement("div", {
    className: "ms-detail__empty"
  }, "Visual instruction editor \u2014 author actions, options, tiers & dependency rules."), tab === "raw" && /*#__PURE__*/React.createElement(RawPanel, {
    mod: focused
  })), /*#__PURE__*/React.createElement("div", {
    className: "ms-installbar"
  }, /*#__PURE__*/React.createElement("div", {
    className: "ms-installbar__status"
  }, /*#__PURE__*/React.createElement("strong", null, selectedCount), " mods selected", conflictPairs > 0 ? /*#__PURE__*/React.createElement("span", {
    className: "ms-installbar__warn"
  }, " \xB7 ", conflictPairs, " conflict", conflictPairs > 1 ? "s" : "", " to resolve") : /*#__PURE__*/React.createElement("span", {
    className: "ms-installbar__ok"
  }, " \xB7 no conflicts")), /*#__PURE__*/React.createElement(Button, {
    variant: "primary",
    size: "lg",
    disabled: conflictPairs > 0 || selectedCount === 0,
    onClick: onInstall
  }, "Install All"))));
}
function RawPanel({
  mod
}) {
  if (!mod) return /*#__PURE__*/React.createElement("div", {
    className: "ms-detail__empty"
  }, "Select a mod to view its raw TOML.");
  return /*#__PURE__*/React.createElement("div", {
    className: "ms-scroll"
  }, /*#__PURE__*/React.createElement("pre", {
    className: "ms-raw"
  }, /*#__PURE__*/React.createElement("code", null, /*#__PURE__*/React.createElement("span", {
    className: "ms-h"
  }, "[[component]]"), "\n", /*#__PURE__*/React.createElement("span", {
    className: "ms-k"
  }, "name"), " = ", /*#__PURE__*/React.createElement("span", {
    className: "ms-name"
  }, "\"", mod.name, "\""), "\n", /*#__PURE__*/React.createElement("span", {
    className: "ms-k"
  }, "author"), " = ", /*#__PURE__*/React.createElement("span", {
    className: "ms-auth"
  }, "\"", mod.author, "\""), "\n", /*#__PURE__*/React.createElement("span", {
    className: "ms-k"
  }, "category"), " = ", /*#__PURE__*/React.createElement("span", {
    className: "ms-cat"
  }, "\"", mod.category, "\""), "\n", /*#__PURE__*/React.createElement("span", {
    className: "ms-k"
  }, "tier"), " = ", /*#__PURE__*/React.createElement("span", {
    className: "ms-desc"
  }, "\"", mod.tier, "\""), "\n", /*#__PURE__*/React.createElement("span", {
    className: "ms-k"
  }, "is_selected"), " = ", /*#__PURE__*/React.createElement("span", {
    className: "ms-val"
  }, String(!!mod.selected)), "\n", (mod.instructions || []).map((ins, i) => `\n[[component.instructions]]\naction = "${ins.action}"\nsource = ["${ins.source}"]${ins.dest ? `\ndestination = "${ins.dest}"` : ""}`).join("\n"))));
}
window.Workspace = Workspace;
})(); } catch (e) { __ds_ns.__errors.push({ path: "ui_kits/modsync-app/Workspace.jsx", error: String((e && e.message) || e) }); }

// ui_kits/modsync-app/data.js
try { (() => {
// ModSync UI kit — sample mod data, lifted from the repo's sample_mods.toml
window.MODSYNC_DATA = {
  instructionFile: "kotor1_essential_build.toml",
  mods: [{
    id: "k1cp",
    name: "KOTOR 1 Community Patch",
    author: "Multiple Contributors",
    category: "Bug Fix",
    tier: "Essential",
    selected: true,
    description: "A comprehensive bugfix patch for KOTOR 1 that addresses numerous issues found in the game. Install this first — other mods build on a stable base.",
    links: ["deadlystream.com/files/1258", "nexusmods.com/kotor/mods/1258"],
    instructions: [{
      action: "Extract",
      source: "K1CP_v1.10.0.zip",
      dest: "<<kotorDirectory>>"
    }, {
      action: "Copy",
      source: "K1CP Files/*",
      dest: "<<kotorDirectory>>"
    }]
  }, {
    id: "ai",
    name: "Improved AI",
    author: "stoffe",
    category: "Gameplay",
    tier: "Recommended",
    selected: false,
    dependencies: ["KOTOR 1 Community Patch"],
    description: "Enhances the artificial intelligence of NPCs and companions in combat situations.",
    links: ["deadlystream.com/files/1001"],
    instructions: [{
      action: "TSLPatcher",
      source: "tslpatchdata",
      dest: "<<kotorDirectory>>",
      args: "0"
    }]
  }, {
    id: "hdui",
    name: "HD UI Elements",
    author: "ndix UR",
    category: "Graphics",
    tier: "Optional",
    selected: false,
    description: "High-definition user interface elements for modern displays.",
    links: ["deadlystream.com/files/1159"],
    options: ["High Resolution (2K)", "Standard Resolution (1K)"],
    instructions: [{
      action: "Copy",
      source: "HD_UI_Elements/*",
      dest: "<<kotorDirectory>>"
    }]
  }, {
    id: "bastila",
    name: "Bastila Romance Enhancement",
    author: "jc2",
    category: "Story",
    tier: "Optional",
    selected: false,
    conflicts: ["Alternative Romance Mod"],
    description: "Expands and improves the romance storyline with Bastila Shan.",
    links: ["deadlystream.com/files/1999"],
    instructions: [{
      action: "TSLPatcher",
      source: "Bastila_Romance_TSLPatchdata",
      dest: "<<kotorDirectory>>"
    }]
  }, {
    id: "altromance",
    name: "Alternative Romance Mod",
    author: "Revan's Exile",
    category: "Story",
    tier: "Optional",
    selected: false,
    conflicts: ["Bastila Romance Enhancement"],
    status: "warning",
    description: "An alternative approach to romance in KOTOR with different characters.",
    links: ["deadlystream.com/files/2001"],
    instructions: [{
      action: "Copy",
      source: "Alternative_Romance/*",
      dest: "<<kotorDirectory>>"
    }, {
      action: "Execute",
      source: "setup.exe",
      args: "--silent"
    }]
  }],
  actionColor: {
    Extract: "var(--action-extract)",
    Copy: "var(--action-copy)",
    TSLPatcher: "var(--action-tslpatcher)",
    Execute: "var(--action-execute)",
    Delete: "var(--action-delete)"
  }
};
})(); } catch (e) { __ds_ns.__errors.push({ path: "ui_kits/modsync-app/data.js", error: String((e && e.message) || e) }); }

__ds_ns.Badge = __ds_scope.Badge;

__ds_ns.Button = __ds_scope.Button;

__ds_ns.Card = __ds_scope.Card;

__ds_ns.ProgressBar = __ds_scope.ProgressBar;

__ds_ns.Checkbox = __ds_scope.Checkbox;

__ds_ns.Select = __ds_scope.Select;

__ds_ns.TextInput = __ds_scope.TextInput;

__ds_ns.ToggleSwitch = __ds_scope.ToggleSwitch;

__ds_ns.ModListItem = __ds_scope.ModListItem;

__ds_ns.ThemeSwitcher = __ds_scope.ThemeSwitcher;

__ds_ns.Tabs = __ds_scope.Tabs;

})();


/* ------------------------------------------------------------------
   ModSync Marketplace layer — ratings, stars, reviews, listing cards and
   the wired Marketplace surface. Plain React.createElement (classic
   script, no JSX). Appended to the core bundle; exports attach to the
   same window namespace.
   ------------------------------------------------------------------ */
(() => {
const __ds_ns = (window.ModSyncDesignSystem_2b35e3 = window.ModSyncDesignSystem_2b35e3 || {});
(__ds_ns.__errors = __ds_ns.__errors || []);
const React = window.React;
if (!React) { __ds_ns.__errors.push({ path: "marketplace", error: "window.React missing" }); return; }
const h = React.createElement;
const { useState, useMemo, useEffect, useRef, useCallback } = React;

function inject(id, css) {
  if (typeof document === "undefined") return;
  if (document.getElementById(id)) return;
  const s = document.createElement("style"); s.id = id; s.textContent = css; document.head.appendChild(s);
}
const cx = (...a) => a.filter(Boolean).join(" ");
const fmtCount = (n) => n >= 1e6 ? (n / 1e6).toFixed(1).replace(/\.0$/, "") + "M" : n >= 1e3 ? (n / 1e3).toFixed(n >= 1e4 ? 0 : 1).replace(/\.0$/, "") + "K" : String(n);
const fmtDate = (iso) => { try { return new Date(iso).toLocaleDateString(undefined, { year: "numeric", month: "short", day: "numeric" }); } catch { return iso; } };
const initials = (name) => (name || "?").split(/\s+/).map(w => w[0]).join("").slice(0, 2).toUpperCase();

/* ================================================================
   StarRating — display or interactive five-star control
   ================================================================ */
const STAR_CSS = `
.ms-stars{font-family:var(--font-default);display:inline-flex;align-items:center;gap:6px;color:var(--text-secondary);font-size:var(--text-xs);line-height:1}
.ms-stars__row{display:inline-flex;gap:2px}
.ms-stars__star{position:relative;display:inline-block;width:var(--ms-star,16px);height:var(--ms-star,16px);flex:none}
.ms-stars__star svg{position:absolute;inset:0;width:100%;height:100%;display:block}
.ms-stars__star .ms-stars__empty{fill:none;stroke:var(--rating-star-empty);stroke-width:1.6}
.ms-stars__star .ms-stars__fill{fill:var(--rating-star);clip-path:inset(0 calc(100% - var(--ms-fill,0%)) 0 0)}
.ms-stars--sm{--ms-star:12px}.ms-stars--md{--ms-star:16px}.ms-stars--lg{--ms-star:22px}.ms-stars--xl{--ms-star:28px}
.ms-stars__value{font-weight:var(--weight-semibold);color:var(--text-primary)}
.ms-stars__count{color:var(--text-secondary)}
.ms-stars--input{cursor:pointer}
.ms-stars--input .ms-stars__btn{appearance:none;background:none;border:0;padding:0;margin:0;cursor:pointer;display:inline-flex;border-radius:2px}
.ms-stars--input .ms-stars__btn:focus-visible{outline:none;box-shadow:var(--focus-ring)}
.ms-stars--input .ms-stars__btn:hover .ms-stars__fill{filter:brightness(1.15)}
.ms-stars__label{min-width:7em;color:var(--text-secondary)}
`;
const STAR_PATH = "M12 2.5l2.9 6.1 6.7.8-4.9 4.6 1.3 6.6L12 17.3 6 20.6l1.3-6.6L2.4 9.4l6.7-.8z";
function StarGlyph({ fill }) {
  return h("span", { className: "ms-stars__star", style: { "--ms-fill": `${Math.max(0, Math.min(100, fill))}%` }, "aria-hidden": true },
    h("svg", { viewBox: "0 0 24 24" }, h("path", { className: "ms-stars__empty", d: STAR_PATH })),
    h("svg", { viewBox: "0 0 24 24" }, h("path", { className: "ms-stars__fill", d: STAR_PATH }))
  );
}
const RATING_WORDS = ["", "Poor", "Fair", "Good", "Great", "Excellent"];
function StarRating({ value = 0, count, size = "md", onChange, showValue = false, showCount = true, labels = false, name = "rating", className = "", ...rest }) {
  inject("ms-stars-css", STAR_CSS);
  const [hover, setHover] = useState(0);
  const interactive = typeof onChange === "function";
  const shown = interactive && hover ? hover : value;
  const stars = [1, 2, 3, 4, 5].map((i) => {
    const fill = Math.round(Math.max(0, Math.min(1, shown - (i - 1))) * 100);
    if (!interactive) return h(StarGlyph, { key: i, fill });
    return h("button", {
      key: i, type: "button", className: "ms-stars__btn", role: "radio", "aria-checked": value === i, "aria-label": `${i} star${i > 1 ? "s" : ""}`,
      onMouseEnter: () => setHover(i), onMouseLeave: () => setHover(0), onFocus: () => setHover(i), onBlur: () => setHover(0),
      onClick: () => onChange(i),
      onKeyDown: (e) => { if (e.key === "ArrowRight" || e.key === "ArrowUp") { e.preventDefault(); onChange(Math.min(5, (value || 0) + 1)); } if (e.key === "ArrowLeft" || e.key === "ArrowDown") { e.preventDefault(); onChange(Math.max(1, (value || 1) - 1)); } },
    }, h(StarGlyph, { fill }));
  });
  return h("span", { className: cx("ms-stars", `ms-stars--${size}`, interactive && "ms-stars--input", className), role: interactive ? "radiogroup" : "img", "aria-label": interactive ? name : `${Number(value).toFixed(1)} out of 5 stars${count != null ? `, ${count} ratings` : ""}`, ...rest },
    h("span", { className: "ms-stars__row" }, stars),
    showValue ? h("span", { className: "ms-stars__value" }, Number(value).toFixed(1)) : null,
    showCount && count != null ? h("span", { className: "ms-stars__count" }, `(${fmtCount(count)})`) : null,
    labels && interactive ? h("span", { className: "ms-stars__label" }, RATING_WORDS[shown] || "Pick a rating") : null
  );
}

/* ================================================================
   RatingSummary — average, total and the 5→1 distribution
   ================================================================ */
const SUMMARY_CSS = `
.ms-rsum{font-family:var(--font-default);display:grid;grid-template-columns:auto 1fr;gap:18px 24px;align-items:center;color:var(--text-primary)}
.ms-rsum__avg{display:flex;flex-direction:column;align-items:center;gap:4px;min-width:96px}
.ms-rsum__big{font-size:40px;font-weight:var(--weight-bold);line-height:1;color:var(--text-heading)}
.ms-rsum__total{font-size:var(--text-xs);color:var(--text-secondary)}
.ms-rsum__bars{display:flex;flex-direction:column;gap:5px;min-width:180px}
.ms-rsum__bar{appearance:none;border:0;background:none;padding:2px 4px;margin:0 -4px;display:grid;grid-template-columns:3.2em 1fr 3.2em;align-items:center;gap:10px;font:inherit;font-size:var(--text-xs);color:var(--text-secondary);cursor:pointer;border-radius:var(--radius-sm);text-align:left}
.ms-rsum__bar:hover{background:var(--surface-hover);color:var(--text-primary)}
.ms-rsum__bar:focus-visible{outline:none;box-shadow:var(--focus-ring)}
.ms-rsum__bar--active{color:var(--brand-accent)}
.ms-rsum__track{height:8px;border-radius:var(--radius-pill);background:var(--rating-bar-track);overflow:hidden;border:1px solid var(--border-default)}
.ms-rsum__fillbar{height:100%;background:var(--rating-star);border-radius:inherit;transition:width var(--transition)}
.ms-rsum__pct{text-align:right;font-variant-numeric:tabular-nums}
.ms-rsum--compact{grid-template-columns:1fr;gap:10px}
`;
function RatingSummary({ average = 0, distribution = [0, 0, 0, 0, 0], activeStar = null, onSelectStar, compact = false, className = "", ...rest }) {
  inject("ms-rsum-css", SUMMARY_CSS);
  const total = distribution.reduce((a, b) => a + b, 0);
  return h("div", { className: cx("ms-rsum", compact && "ms-rsum--compact", className), ...rest },
    h("div", { className: "ms-rsum__avg" },
      h("div", { className: "ms-rsum__big" }, total ? average.toFixed(1) : "–"),
      h(StarRating, { value: average, size: "md", showCount: false }),
      h("div", { className: "ms-rsum__total" }, total ? `${fmtCount(total)} rating${total === 1 ? "" : "s"}` : "No ratings yet")
    ),
    h("div", { className: "ms-rsum__bars", role: "list" },
      [5, 4, 3, 2, 1].map((star) => {
        const n = distribution[star - 1] || 0; const pct = total ? Math.round((n / total) * 100) : 0;
        return h("button", { key: star, type: "button", role: "listitem", className: cx("ms-rsum__bar", activeStar === star && "ms-rsum__bar--active"), "aria-pressed": activeStar === star, "aria-label": `${star} star: ${n} ratings, ${pct}%`, onClick: onSelectStar ? () => onSelectStar(activeStar === star ? null : star) : undefined, disabled: !onSelectStar },
          h("span", null, `${star} ★`),
          h("span", { className: "ms-rsum__track" }, h("span", { className: "ms-rsum__fillbar", style: { width: `${pct}%` } })),
          h("span", { className: "ms-rsum__pct" }, `${pct}%`)
        );
      })
    )
  );
}

/* ================================================================
   ReviewCard — one review; `own` adds edit/delete
   ================================================================ */
const REVIEW_CSS = `
.ms-review{font-family:var(--font-default);display:grid;grid-template-columns:36px 1fr;gap:12px;padding:14px 16px;background:var(--surface-card);border:var(--border-thin) solid var(--border-default);border-radius:var(--radius-md);color:var(--text-primary)}
.ms-review--own{border-color:var(--brand-primary);background:var(--selected-bg)}
.ms-review__avatar{width:36px;height:36px;border-radius:var(--radius-pill);background:var(--surface-raised);border:var(--border-thin) solid var(--border-default);display:flex;align-items:center;justify-content:center;font-size:var(--text-sm);font-weight:var(--weight-bold);color:var(--brand-accent)}
.ms-review__head{display:flex;flex-wrap:wrap;align-items:center;gap:6px 10px;font-size:var(--text-sm)}
.ms-review__author{font-weight:var(--weight-semibold)}
.ms-review__meta{color:var(--text-secondary);font-size:var(--text-2xs)}
.ms-review__chip{display:inline-flex;align-items:center;gap:4px;font-size:var(--text-2xs);font-weight:var(--weight-bold);letter-spacing:.03em;padding:2px 7px;border-radius:var(--radius-sm);border:1px solid}
.ms-review__chip--verified{color:var(--verified);border-color:var(--verified)}
.ms-review__chip--own{color:var(--brand-primary);border-color:var(--brand-primary)}
.ms-review__title{font-weight:var(--weight-semibold);font-size:var(--text-md);margin:6px 0 2px}
.ms-review__body{font-size:var(--text-sm);line-height:var(--leading-normal);white-space:pre-wrap;word-break:break-word}
.ms-review__foot{display:flex;flex-wrap:wrap;align-items:center;gap:8px;margin-top:10px}
.ms-review__act{appearance:none;font:inherit;font-size:var(--text-2xs);background:var(--surface-card);border:var(--border-thin) solid var(--border-default);border-radius:var(--radius-sm);color:var(--text-primary);padding:4px 9px;cursor:pointer;display:inline-flex;align-items:center;gap:5px;transition:border-color var(--transition),color var(--transition)}
.ms-review__act:hover{border-color:var(--brand-accent);color:var(--brand-accent)}
.ms-review__act:focus-visible{outline:none;box-shadow:var(--focus-ring)}
.ms-review__act--on{border-color:var(--brand-primary);color:var(--brand-primary)}
.ms-review__act--danger:hover{border-color:var(--restriction);color:var(--restriction)}
.ms-review__reply{margin-top:10px;padding:10px 12px;border-left:3px solid var(--brand-accent);background:var(--surface-raised);border-radius:0 var(--radius-sm) var(--radius-sm) 0;font-size:var(--text-sm)}
.ms-review__reply b{color:var(--text-heading);font-size:var(--text-2xs);letter-spacing:.04em;text-transform:uppercase;display:block;margin-bottom:4px}
`;
function ReviewCard({ review, own = false, onHelpful, onEdit, onDelete, className = "", ...rest }) {
  inject("ms-review-css", REVIEW_CSS);
  const r = review || {};
  return h("article", { className: cx("ms-review", own && "ms-review--own", className), ...rest },
    h("div", { className: "ms-review__avatar", "aria-hidden": true }, initials(r.author)),
    h("div", null,
      h("div", { className: "ms-review__head" },
        h("span", { className: "ms-review__author" }, r.author || "Anonymous"),
        own ? h("span", { className: "ms-review__chip ms-review__chip--own" }, "Your review") : null,
        r.verified ? h("span", { className: "ms-review__chip ms-review__chip--verified", title: "This reviewer installed the mod through ModSync" }, "✓ Verified install") : null,
        h(StarRating, { value: r.rating || 0, size: "sm", showCount: false }),
        h("span", { className: "ms-review__meta" }, [r.version ? `v${r.version}` : null, r.date ? fmtDate(r.date) : null, r.edited ? "edited" : null].filter(Boolean).join(" · "))
      ),
      r.title ? h("div", { className: "ms-review__title" }, r.title) : null,
      h("div", { className: "ms-review__body" }, r.body),
      r.reply ? h("div", { className: "ms-review__reply" }, h("b", null, `Response from ${r.reply.author || "the author"}`), r.reply.body) : null,
      h("div", { className: "ms-review__foot" },
        onHelpful ? h("button", { type: "button", className: cx("ms-review__act", r.helpfulByMe && "ms-review__act--on"), "aria-pressed": !!r.helpfulByMe, onClick: () => onHelpful(r) }, "👍 Helpful", h("span", null, `(${r.helpful || 0})`)) : h("span", { className: "ms-review__meta" }, `${r.helpful || 0} found this helpful`),
        own && onEdit ? h("button", { type: "button", className: "ms-review__act", onClick: () => onEdit(r) }, "✏️ Edit") : null,
        own && onDelete ? h("button", { type: "button", className: "ms-review__act ms-review__act--danger", onClick: () => onDelete(r) }, "Delete") : null
      )
    )
  );
}

/* ================================================================
   ReviewComposer — write or edit your own review
   ================================================================ */
const COMPOSER_CSS = `
.ms-compose{font-family:var(--font-default);display:flex;flex-direction:column;gap:12px;padding:16px;background:var(--surface-raised);border:var(--border-thin) solid var(--border-default);border-radius:var(--radius-lg);color:var(--text-primary)}
.ms-compose__title{font-size:var(--text-lg);font-weight:var(--weight-bold);color:var(--text-heading)}
.ms-compose__row{display:flex;flex-wrap:wrap;align-items:center;gap:10px 14px;font-size:var(--text-sm)}
.ms-compose label{font-size:var(--text-xs);color:var(--text-secondary);display:flex;flex-direction:column;gap:5px}
.ms-compose input[type=text],.ms-compose textarea{font-family:var(--font-default);font-size:var(--text-md);color:var(--text-primary);background:var(--surface-card);border:var(--border-base) solid var(--border-default);border-radius:var(--radius-sm);padding:8px 10px;caret-color:var(--caret);transition:border-color var(--transition)}
.ms-compose input[type=text]:hover,.ms-compose textarea:hover{border-color:var(--brand-accent)}
.ms-compose input[type=text]:focus-visible,.ms-compose textarea:focus-visible{outline:none;border-color:var(--brand-primary);box-shadow:var(--focus-ring)}
.ms-compose textarea{min-height:110px;resize:vertical;line-height:var(--leading-normal)}
.ms-compose__hint{display:flex;justify-content:space-between;font-size:var(--text-2xs);color:var(--text-secondary)}
.ms-compose__hint--bad{color:var(--error)}
.ms-compose__check{display:flex;flex-direction:row;align-items:center;gap:8px;font-size:var(--text-sm);color:var(--text-primary)}
.ms-compose__actions{display:flex;gap:8px;justify-content:flex-end;flex-wrap:wrap}
.ms-compose__err{font-size:var(--text-xs);color:var(--error);border:1px solid var(--error);border-radius:var(--radius-sm);padding:6px 10px}
`;
const MIN_BODY = 20, MAX_BODY = 2000;
function ReviewComposer({ initial, version, onSubmit, onCancel, className = "", ...rest }) {
  inject("ms-compose-css", COMPOSER_CSS);
  const Button = __ds_ns.Button;
  const [rating, setRating] = useState(initial?.rating || 0);
  const [title, setTitle] = useState(initial?.title || "");
  const [body, setBody] = useState(initial?.body || "");
  const [verified, setVerified] = useState(initial?.verified ?? true);
  const [touched, setTouched] = useState(false);
  const bodyOk = body.trim().length >= MIN_BODY && body.length <= MAX_BODY;
  const errors = [];
  if (!rating) errors.push("Pick a star rating.");
  if (!bodyOk) errors.push(`Write at least ${MIN_BODY} characters (max ${MAX_BODY}).`);
  const submit = (e) => { e.preventDefault(); setTouched(true); if (errors.length) return; onSubmit && onSubmit({ rating, title: title.trim(), body: body.trim(), verified, version }); };
  const btn = (props, label) => Button ? h(Button, props, label) : h("button", { type: props.type || "button", onClick: props.onClick, className: "ms-review__act" }, label);
  return h("form", { className: cx("ms-compose", className), onSubmit: submit, noValidate: true, ...rest },
    h("div", { className: "ms-compose__title" }, initial ? "Edit your review" : "Write a review"),
    h("div", { className: "ms-compose__row" },
      h("span", { style: { color: "var(--text-secondary)", fontSize: "var(--text-xs)" } }, "Your rating"),
      h(StarRating, { value: rating, onChange: setRating, size: "lg", labels: true, name: "Your rating" })
    ),
    h("label", null, "Title (optional)", h("input", { type: "text", maxLength: 80, value: title, onChange: (e) => setTitle(e.target.value), placeholder: "Sum it up in a few words" })),
    h("label", null, "Review",
      h("textarea", { value: body, maxLength: MAX_BODY, onChange: (e) => setBody(e.target.value), placeholder: "What worked, what didn't, which game version you used…", "aria-invalid": touched && !bodyOk }),
      h("span", { className: cx("ms-compose__hint", touched && !bodyOk && "ms-compose__hint--bad") }, h("span", null, version ? `Reviewing v${version}` : ""), h("span", null, `${body.trim().length} / ${MAX_BODY}`))
    ),
    h("label", { className: "ms-compose__check" }, h("input", { type: "checkbox", checked: verified, onChange: (e) => setVerified(e.target.checked) }), "I installed this mod through ModSync (shows a Verified install badge)"),
    touched && errors.length ? h("div", { className: "ms-compose__err", role: "alert" }, errors.join(" ")) : null,
    h("div", { className: "ms-compose__actions" },
      onCancel ? btn({ variant: "ghost", onClick: onCancel }, "Cancel") : null,
      btn({ variant: "primary", type: "submit" }, initial ? "Save Changes" : "Post Review")
    )
  );
}

/* ================================================================
   InstallButton — idle → installing → installed / update
   ================================================================ */
const INSTALL_CSS = `
.ms-install{font-family:var(--font-default);position:relative;display:inline-flex;align-items:center;justify-content:center;gap:7px;min-width:104px;height:32px;padding:0 14px;border-radius:var(--radius-sm);border:var(--border-base) solid var(--brand-primary);background:var(--brand-primary);color:var(--text-on-accent);font-size:var(--text-sm);font-weight:var(--weight-semibold);cursor:pointer;overflow:hidden;transition:background var(--transition),border-color var(--transition),color var(--transition);white-space:nowrap}
.ms-install:hover{background:var(--brand-primary-bright);border-color:var(--brand-primary-bright)}
.ms-install:focus-visible{outline:none;box-shadow:var(--focus-ring)}
.ms-install--installed{background:var(--surface-card);color:var(--success);border-color:var(--success);cursor:default}
.ms-install--installed:hover{background:var(--surface-card);border-color:var(--success)}
.ms-install--update{background:var(--surface-card);color:var(--brand-accent);border-color:var(--brand-accent)}
.ms-install--update:hover{background:var(--surface-hover);border-color:var(--brand-accent)}
.ms-install--busy{background:var(--surface-card);color:var(--text-primary);border-color:var(--border-default);cursor:progress}
.ms-install__bar{position:absolute;left:0;top:0;bottom:0;background:var(--selected-bg);transition:width .2s linear;z-index:0}
.ms-install__lbl{position:relative;z-index:1}
.ms-install--sm{height:26px;min-width:84px;padding:0 10px;font-size:var(--text-2xs)}
`;
function InstallButton({ state = "idle", progress = 0, size = "md", onInstall, onUpdate, onUninstall, className = "", ...rest }) {
  inject("ms-install-css", INSTALL_CSS);
  const label = state === "installing" ? `Installing ${Math.round(progress)}%` : state === "installed" ? "✓ Installed" : state === "update" ? "⬆ Update" : "Install";
  const onClick = state === "idle" ? onInstall : state === "update" ? onUpdate : state === "installed" ? onUninstall : undefined;
  return h("button", { type: "button", className: cx("ms-install", `ms-install--${size}`, state === "installing" && "ms-install--busy", state === "installed" && "ms-install--installed", state === "update" && "ms-install--update", className), onClick, "aria-busy": state === "installing", "aria-live": "polite", title: state === "installed" && onUninstall ? "Click to uninstall" : undefined, ...rest },
    state === "installing" ? h("span", { className: "ms-install__bar", style: { width: `${progress}%` } }) : null,
    h("span", { className: "ms-install__lbl" }, label)
  );
}

/* ================================================================
   ModCard — marketplace listing (grid or list layout)
   ================================================================ */
const MODCARD_CSS = `
.ms-mc{font-family:var(--font-default);display:grid;grid-template-columns:48px 1fr;gap:12px;padding:14px;background:var(--surface-card);border:var(--border-thin) solid var(--border-default);border-radius:var(--radius-lg);color:var(--text-primary);cursor:pointer;transition:border-color var(--transition),background var(--transition);min-width:0;text-align:left;font:inherit}
.ms-mc:hover{border-color:var(--brand-accent);background:var(--surface-hover)}
.ms-mc:focus-visible{outline:none;box-shadow:var(--focus-ring)}
.ms-mc--selected{border-color:var(--brand-primary);background:var(--selected-bg)}
.ms-mc--list{grid-template-columns:40px 1fr auto;align-items:center;padding:10px 12px;border-radius:var(--radius-sm)}
.ms-mc__icon{width:48px;height:48px;border-radius:var(--radius-md);background:var(--surface-raised);border:var(--border-thin) solid var(--border-default);display:flex;align-items:center;justify-content:center;font-size:24px;line-height:1}
.ms-mc--list .ms-mc__icon{width:40px;height:40px;font-size:20px}
.ms-mc__main{min-width:0;display:flex;flex-direction:column;gap:5px}
.ms-mc__top{display:flex;align-items:baseline;gap:8px;min-width:0}
.ms-mc__name{font-size:var(--text-md);font-weight:var(--weight-semibold);white-space:nowrap;overflow:hidden;text-overflow:ellipsis}
.ms-mc__ver{font-size:var(--text-2xs);color:var(--text-secondary);font-family:var(--font-mono)}
.ms-mc__author{font-size:var(--text-2xs);color:var(--text-secondary);display:flex;align-items:center;gap:4px}
.ms-mc__verified{color:var(--verified);font-weight:var(--weight-bold)}
.ms-mc__desc{font-size:var(--text-sm);color:var(--text-primary);opacity:.85;line-height:var(--leading-snug);display:-webkit-box;-webkit-line-clamp:2;-webkit-box-orient:vertical;overflow:hidden}
.ms-mc--list .ms-mc__desc{-webkit-line-clamp:1}
.ms-mc__stats{display:flex;flex-wrap:wrap;align-items:center;gap:6px 12px;font-size:var(--text-2xs);color:var(--text-secondary);margin-top:2px}
.ms-mc__stat{display:inline-flex;align-items:center;gap:4px;white-space:nowrap}
.ms-mc__badges{display:flex;flex-wrap:wrap;gap:5px;margin-top:2px}
.ms-mc__side{display:flex;flex-direction:column;align-items:flex-end;gap:8px}
.ms-mc__starbtn{appearance:none;background:none;border:var(--border-thin) solid transparent;border-radius:var(--radius-sm);color:var(--text-secondary);font:inherit;font-size:var(--text-2xs);padding:3px 7px;cursor:pointer;display:inline-flex;align-items:center;gap:4px}
.ms-mc__starbtn:hover{color:var(--brand-accent);border-color:var(--border-default)}
.ms-mc__starbtn--on{color:var(--rating-star)}
.ms-mc__starbtn:focus-visible{outline:none;box-shadow:var(--focus-ring)}
.ms-mc__foot{display:flex;justify-content:space-between;align-items:center;gap:8px;margin-top:4px}
`;
function ModCard({ mod, layout = "grid", selected = false, installState = "idle", installProgress = 0, starred = false, onSelect, onInstall, onUpdate, onToggleStar, className = "", ...rest }) {
  inject("ms-mc-css", MODCARD_CSS);
  const Badge = __ds_ns.Badge;
  const m = mod || {};
  const stop = (fn) => (e) => { e.stopPropagation(); fn && fn(m); };
  const stats = h("div", { className: "ms-mc__stats" },
    h("span", { className: "ms-mc__stat", title: `${m.ratingCount || 0} ratings` }, h(StarRating, { value: m.rating || 0, size: "sm", showCount: false }), h("b", null, (m.rating || 0).toFixed(1)), `(${fmtCount(m.ratingCount || 0)})`),
    h("span", { className: "ms-mc__stat", title: "Installs" }, "⬇", fmtCount(m.installs || 0)),
    h("span", { className: "ms-mc__stat", title: "Stars (endorsements)" }, "★", fmtCount((m.stars || 0) + (starred ? 1 : 0))),
    m.updated ? h("span", { className: "ms-mc__stat", title: "Last updated" }, "↻", fmtDate(m.updated)) : null
  );
  const badges = Badge ? h("div", { className: "ms-mc__badges" },
    m.category ? h(Badge, { variant: "category" }, m.category) : null,
    m.tier ? h(Badge, { variant: "tier" }, m.tier) : null,
    (m.games || []).map((g) => h(Badge, { key: g, variant: "neutral" }, g))
  ) : null;
  const starBtn = h("button", { type: "button", className: cx("ms-mc__starbtn", starred && "ms-mc__starbtn--on"), "aria-pressed": starred, title: starred ? "Remove star" : "Star this mod", onClick: stop(onToggleStar) }, starred ? "★ Starred" : "☆ Star");
  const install = h(InstallButton, { size: "sm", state: installState, progress: installProgress, onInstall: stop(onInstall), onUpdate: stop(onUpdate), onClick: undefined });
  return h("div", { role: "button", tabIndex: 0, className: cx("ms-mc", layout === "list" && "ms-mc--list", selected && "ms-mc--selected", className), "aria-pressed": selected, onClick: () => onSelect && onSelect(m), onKeyDown: (e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); onSelect && onSelect(m); } }, ...rest },
    h("div", { className: "ms-mc__icon", "aria-hidden": true }, m.icon || "📦"),
    h("div", { className: "ms-mc__main" },
      h("div", { className: "ms-mc__top" }, h("span", { className: "ms-mc__name" }, m.name), m.version ? h("span", { className: "ms-mc__ver" }, `v${m.version}`) : null),
      h("div", { className: "ms-mc__author" }, "by ", h("span", null, m.author), m.authorVerified ? h("span", { className: "ms-mc__verified", title: "Verified author" }, "✓") : null),
      h("div", { className: "ms-mc__desc" }, m.summary),
      stats,
      layout === "grid" ? badges : null,
      layout === "grid" ? h("div", { className: "ms-mc__foot" }, starBtn, install) : null
    ),
    layout === "list" ? h("div", { className: "ms-mc__side" }, install, starBtn) : null
  );
}

/* ================================================================
   Marketplace — the fully wired browse → detail → review surface
   ================================================================ */
const MARKET_CSS = `
.ms-mk{font-family:var(--font-default);display:grid;grid-template-columns:220px minmax(0,1fr) minmax(0,460px);grid-template-rows:auto 1fr;height:100%;min-height:640px;background:var(--surface-bg);color:var(--text-primary);border:var(--border-thin) solid var(--border-default);overflow:hidden}
.ms-mk__top{grid-column:1/-1;display:flex;align-items:center;gap:14px;padding:10px 16px;background:var(--surface-raised);border-bottom:var(--border-thin) solid var(--border-default)}
.ms-mk__brand{display:flex;align-items:center;gap:8px;font-size:var(--text-xl);font-weight:var(--weight-bold);color:var(--text-primary);white-space:nowrap}
.ms-mk__brand small{font-size:var(--text-xs);font-weight:var(--weight-normal);color:var(--text-secondary)}
.ms-mk__search{flex:1;display:flex;align-items:center;gap:8px;background:var(--surface-card);border:var(--border-base) solid var(--border-default);border-radius:var(--radius-sm);padding:6px 10px;max-width:520px}
.ms-mk__search:focus-within{border-color:var(--brand-primary);box-shadow:var(--focus-ring)}
.ms-mk__search input{flex:1;background:none;border:0;outline:none;font:inherit;font-size:var(--text-md);color:var(--text-primary);caret-color:var(--caret);min-width:0}
.ms-mk__search input::placeholder{color:var(--text-secondary)}
.ms-mk__topr{margin-left:auto;display:flex;align-items:center;gap:10px}
.ms-mk__side{border-right:var(--border-thin) solid var(--border-default);background:var(--surface-card);padding:14px 10px;display:flex;flex-direction:column;gap:16px;overflow:auto}
.ms-mk__h{font-size:var(--text-2xs);letter-spacing:.06em;text-transform:uppercase;color:var(--text-secondary);padding:0 8px;margin-bottom:6px}
.ms-mk__nav{display:flex;flex-direction:column;gap:2px}
.ms-mk__navb{appearance:none;border:0;background:none;font:inherit;font-size:var(--text-sm);color:var(--text-primary);text-align:left;padding:7px 10px;border-radius:var(--radius-sm);cursor:pointer;display:flex;justify-content:space-between;align-items:center;gap:8px;border-left:3px solid transparent}
.ms-mk__navb:hover{background:var(--surface-hover);color:var(--brand-accent)}
.ms-mk__navb--on{background:var(--selected-bg);border-left-color:var(--brand-primary)}
.ms-mk__navb:focus-visible{outline:none;box-shadow:var(--focus-ring)}
.ms-mk__navb small{color:var(--text-secondary);font-size:var(--text-2xs);font-variant-numeric:tabular-nums}
.ms-mk__seg{display:inline-flex;border:var(--border-thin) solid var(--border-default);border-radius:var(--radius-sm);overflow:hidden}
.ms-mk__seg button{appearance:none;border:0;border-right:var(--border-thin) solid var(--border-default);background:var(--surface-card);color:var(--text-secondary);font:inherit;font-size:var(--text-xs);font-weight:var(--weight-bold);padding:5px 10px;cursor:pointer}
.ms-mk__seg button:last-child{border-right:0}
.ms-mk__seg button:hover{color:var(--brand-accent)}
.ms-mk__seg button[aria-pressed=true]{background:var(--brand-primary);color:var(--text-on-accent)}
.ms-mk__main{display:flex;flex-direction:column;min-width:0;overflow:auto}
.ms-mk__bar{display:flex;align-items:center;gap:12px;padding:12px 16px 8px;font-size:var(--text-xs);color:var(--text-secondary);position:sticky;top:0;background:var(--surface-bg);z-index:1}
.ms-mk__bar select{font:inherit;font-size:var(--text-xs);color:var(--text-primary);background:var(--surface-card);border:var(--border-thin) solid var(--border-default);border-radius:var(--radius-sm);padding:4px 8px}
.ms-mk__grid{display:grid;grid-template-columns:repeat(auto-fill,minmax(260px,1fr));gap:10px;padding:4px 16px 16px}
.ms-mk__list{display:flex;flex-direction:column;gap:6px;padding:4px 16px 16px}
.ms-mk__empty{padding:40px;text-align:center;color:var(--text-secondary);font-size:var(--text-sm)}
.ms-mk__detail{border-left:var(--border-thin) solid var(--border-default);background:var(--surface-card);overflow:auto;display:flex;flex-direction:column}
.ms-mk__dhead{display:grid;grid-template-columns:64px 1fr;gap:14px;padding:18px 18px 12px}
.ms-mk__dicon{width:64px;height:64px;border-radius:var(--radius-lg);background:var(--surface-raised);border:var(--border-thin) solid var(--border-default);display:flex;align-items:center;justify-content:center;font-size:32px}
.ms-mk__dname{font-size:var(--text-2xl);font-weight:var(--weight-bold);color:var(--text-heading);line-height:1.2}
.ms-mk__dby{font-size:var(--text-xs);color:var(--text-secondary);margin-top:2px}
.ms-mk__dstats{display:flex;flex-wrap:wrap;gap:6px 14px;font-size:var(--text-xs);color:var(--text-secondary);margin-top:8px}
.ms-mk__dstats b{color:var(--text-primary)}
.ms-mk__dact{display:flex;flex-wrap:wrap;gap:8px;padding:0 18px 12px;align-items:center}
.ms-mk__dbody{padding:14px 18px 24px;display:flex;flex-direction:column;gap:14px;font-size:var(--text-sm);line-height:var(--leading-normal)}
.ms-mk__dbody h4{margin:0;font-size:var(--text-xs);letter-spacing:.06em;text-transform:uppercase;color:var(--text-secondary)}
.ms-mk__kv{display:grid;grid-template-columns:auto 1fr;gap:4px 14px;font-size:var(--text-xs)}
.ms-mk__kv dt{color:var(--text-secondary)}.ms-mk__kv dd{margin:0;font-family:var(--font-mono)}
.ms-mk__rtools{display:flex;flex-wrap:wrap;align-items:center;gap:8px;font-size:var(--text-xs);color:var(--text-secondary)}
.ms-mk__rtools select{font:inherit;font-size:var(--text-xs);color:var(--text-primary);background:var(--surface-card);border:var(--border-thin) solid var(--border-default);border-radius:var(--radius-sm);padding:3px 6px}
.ms-mk__reviews{display:flex;flex-direction:column;gap:8px}
.ms-mk__vers{display:flex;flex-direction:column;gap:6px}
.ms-mk__ver{display:grid;grid-template-columns:auto 1fr auto;gap:10px;align-items:baseline;padding:8px 10px;border:var(--border-thin) solid var(--border-default);border-radius:var(--radius-sm);font-size:var(--text-xs)}
.ms-mk__ver code{font-family:var(--font-mono);color:var(--brand-primary)}
.ms-mk__toast{position:absolute;left:50%;bottom:16px;transform:translateX(-50%);background:var(--surface-raised);border:var(--border-thin) solid var(--brand-accent);border-left-width:3px;color:var(--text-primary);padding:8px 14px;border-radius:var(--radius-sm);font-size:var(--text-xs);box-shadow:var(--shadow-card);z-index:5}
.ms-mk__detail .ms-tab{white-space:nowrap}
.ms-mk__dselect{padding:40px 20px;color:var(--text-secondary);font-size:var(--text-sm);text-align:center}
.ms-mk__link{color:var(--brand-primary);text-decoration:none}.ms-mk__link:hover{color:var(--brand-primary-bright);text-decoration:underline}
@media (max-width:1100px){.ms-mk{grid-template-columns:200px minmax(0,1fr) minmax(0,400px)}}
`;

const SAMPLE_MODS = [
  { id: "k1cp", icon: "🛠️", name: "KOTOR 1 Community Patch", author: "KOTOR Community", authorVerified: true, version: "1.10.0", category: "Bug Fix", tier: "Essential", games: ["K1"], installs: 184203, stars: 9120, updated: "2026-09-02", summary: "The definitive bug-fix compilation: hundreds of script, dialogue and model fixes with no gameplay changes.", description: "A community effort to fix bugs, errors and inconsistencies left in the original game. Install it first: most other mods in the guide assume it is present.\n\nIncludes fixes for broken dialogue triggers, missing animations, placement errors and texture seams. Fully compatible with the widescreen patches.", links: [["Deadly Stream", "https://deadlystream.com/"]], dependencies: [], conflicts: [], versions: [["1.10.0", "2026-09-02", "Adds 40 new fixes; TSLPatcher update."], ["1.9.2", "2025-12-14", "Hotfix for Manaan dialogue loop."], ["1.9.0", "2025-07-01", "Dantooine placement pass."]],
    reviews: [
      { id: "r1", author: "Jaeson T.", rating: 5, verified: true, version: "1.10.0", date: "2026-10-12", title: "Install this first, every time", body: "Fixed the Manaan dialogue loop that soft-locked my last playthrough. Installs cleanly through ModSync with zero conflicts flagged.", helpful: 42 },
      { id: "r2", author: "Mara V.", rating: 4, verified: true, version: "1.9.2", date: "2026-05-21", title: "Great, one caveat", body: "Everything works, but read the warnings: it conflicts with the old 'Yavin IV' mod unless you install that one afterwards. ModSync ordered it correctly for me.", helpful: 17, reply: { author: "KOTOR Community", body: "Thanks! 1.10 adds an explicit restriction entry so ModSync warns up front." } },
      { id: "r3", author: "pyrocat", rating: 3, verified: false, version: "1.9.0", date: "2025-08-03", body: "Solid patch, though I wish the changelog listed which fixes alter cutscenes. Not a dealbreaker.", helpful: 5 },
    ] },
  { id: "wide", icon: "🖥️", name: "Widescreen UI Fix", author: "ndix UR", authorVerified: true, version: "2.2", category: "Graphics", tier: "Essential", games: ["K1", "TSL"], installs: 142880, stars: 7731, updated: "2026-03-18", summary: "Rescaled menus, dialogue boxes and HUD for 16:9 and ultrawide resolutions.", description: "Repositions and rescales every UI panel so nothing is cut off or stretched at modern resolutions. Pair it with the high-resolution menu textures for crisp text.", links: [["Deadly Stream", "https://deadlystream.com/"]], dependencies: [], conflicts: ["Classic 4:3 Menus"], versions: [["2.2", "2026-03-18", "Adds 21:9 and 32:9 layouts."], ["2.1", "2025-02-10", "Fixes the galaxy map at 2560×1440."]],
    reviews: [
      { id: "r4", author: "Dev K.", rating: 5, verified: true, version: "2.2", date: "2026-06-30", title: "Flawless at 3440×1440", body: "Ultrawide finally works without the stretched galaxy map. The ModSync step just copied files and I was done.", helpful: 31 },
      { id: "r5", author: "sithlord99", rating: 2, verified: false, version: "2.1", date: "2025-03-01", body: "Loading screens are still 4:3 letterboxed. Expected them to be covered too. Menus are fine.", helpful: 9 },
    ] },
  { id: "hdtex", icon: "🎨", name: "High-Quality Texture Pack", author: "Sith Holocron", authorVerified: true, version: "4.0.1", category: "Graphics", tier: "Recommended", games: ["K1"], installs: 96410, stars: 5102, updated: "2026-07-22", summary: "Upscaled planetary and character textures that respect the original art direction.", description: "AI-assisted upscales, hand-corrected to remove artifacts. Large download (2.4 GB); ModSync verifies the SHA-256 before extracting.", links: [["Nexus Mods", "https://www.nexusmods.com/"]], dependencies: ["KOTOR 1 Community Patch"], conflicts: [], versions: [["4.0.1", "2026-07-22", "Fixes Taris apartment seam."], ["4.0.0", "2026-05-05", "Full repack, 30% smaller."]],
    reviews: [
      { id: "r6", author: "Mara V.", rating: 5, verified: true, version: "4.0.1", date: "2026-08-02", body: "Looks like the game I remember, not the game that actually shipped. Zero stutter on an RTX card.", helpful: 22 },
      { id: "r7", author: "lowspec_laptop", rating: 3, verified: true, version: "4.0.0", date: "2026-05-30", title: "Heavy on integrated GPUs", body: "Beautiful but my iGPU dropped to 40fps on Taris. The pack could use a 'lite' variant.", helpful: 14, reply: { author: "Sith Holocron", body: "A 2K lite variant is planned for 4.1." } },
      { id: "r8", author: "Jaeson T.", rating: 4, verified: true, version: "4.0.0", date: "2026-05-12", body: "Hash verification caught a corrupted download for me, which is exactly why I use ModSync.", helpful: 8 },
      { id: "r9", author: "nomad", rating: 1, verified: false, version: "3.x", date: "2025-01-15", body: "Old version broke my saves. Don't know if that's still true.", helpful: 2 },
    ] },
  { id: "tslrcm", icon: "📖", name: "TSLRCM", author: "TSLRCM Team", authorVerified: true, version: "1.8.6", category: "Story", tier: "Essential", games: ["TSL"], installs: 221455, stars: 14870, updated: "2025-11-09", summary: "The Sith Lords Restored Content Mod: restores cut content, fixes hundreds of bugs.", description: "Restores dozens of cut scenes, planets and quests the shipped game left unfinished, and fixes the bugs that came with them. Required by nearly every TSL mod.", links: [["Deadly Stream", "https://deadlystream.com/"]], dependencies: [], conflicts: ["Unofficial TSL Patch"], versions: [["1.8.6", "2025-11-09", "Compatibility with the Steam Workshop removal."], ["1.8.5", "2021-01-01", "Malachor fixes."]],
    reviews: [
      { id: "r10", author: "Visas_M", rating: 5, verified: true, version: "1.8.6", date: "2026-02-11", title: "The game as intended", body: "If you only install one TSL mod, this is it. Clean TSLPatcher run through ModSync on Linux, no Wine.", helpful: 88 },
      { id: "r11", author: "HK-47 fan", rating: 5, verified: true, version: "1.8.6", date: "2025-12-20", body: "Droid planet content alone justifies it. Flagged incompatibility with the unofficial patch was accurate.", helpful: 36 },
    ] },
  { id: "m478", icon: "🪐", name: "M4-78 Enhancement Project", author: "Hassat Hunter", authorVerified: false, version: "1.5", category: "Story", tier: "Optional", games: ["TSL"], installs: 58210, stars: 2210, updated: "2024-06-14", summary: "Restores the cut droid planet M4-78 as a full playable area.", description: "An entire restored planet with new quests and voice work. Requires TSLRCM and must be installed after it.", links: [["Deadly Stream", "https://deadlystream.com/"]], dependencies: ["TSLRCM"], conflicts: [], versions: [["1.5", "2024-06-14", "Pathfinding fixes."]],
    reviews: [
      { id: "r12", author: "pyrocat", rating: 4, verified: true, version: "1.5", date: "2025-09-09", body: "Content is uneven but it's a whole planet for free. Install order warnings from ModSync saved me a reinstall.", helpful: 12 },
      { id: "r13", author: "sithlord99", rating: 2, verified: false, version: "1.5", date: "2025-01-28", body: "Too much fetch-questing for my taste. Voice quality varies a lot.", helpful: 6 },
    ] },
  { id: "animfix", icon: "🏃", name: "Animation Fix Pack", author: "DarthParametric", authorVerified: true, version: "1.2.1", category: "Bug Fix", tier: "Recommended", games: ["K1", "TSL"], installs: 71033, stars: 3390, updated: "2026-01-30", summary: "Corrects clipping, floating and frozen animations in combat and cutscenes.", description: "Dozens of model and animation corrections. Installs via TSLPatcher; ModSync snapshots the game directory first so you can roll back.", links: [["Deadly Stream", "https://deadlystream.com/"]], dependencies: ["KOTOR 1 Community Patch"], conflicts: [], versions: [["1.2.1", "2026-01-30", "Fixes lightsaber hilt offset."], ["1.2.0", "2025-10-04", "Adds Twi'lek idle fixes."]],
    reviews: [
      { id: "r14", author: "Dev K.", rating: 4, verified: true, version: "1.2.1", date: "2026-03-03", body: "No more floating Bastila in the Ebon Hawk cutscene. Snapshot restore worked when I installed it in the wrong order.", helpful: 19 },
    ] },
  { id: "music", icon: "🎵", name: "Lossless Music Restoration", author: "Fair Strides", authorVerified: false, version: "0.9-beta", category: "Gameplay", tier: "Optional", games: ["K1"], installs: 12044, stars: 640, updated: "2026-10-28", summary: "Replaces compressed soundtrack files with lossless sources.", description: "Beta. Large files; ModSync verifies each archive hash and warns that the install is not reversible without the snapshot.", links: [["Nexus Mods", "https://www.nexusmods.com/"]], dependencies: [], conflicts: [], versions: [["0.9-beta", "2026-10-28", "Initial public beta."]], reviews: [] },
];
const CATEGORIES = ["All", "Bug Fix", "Gameplay", "Graphics", "Story"];
const STORE_KEY = "modsync.marketplace.v1";
function loadState() { try { const raw = localStorage.getItem(STORE_KEY); return raw ? JSON.parse(raw) : null; } catch { return null; } }
function saveState(s) { try { localStorage.setItem(STORE_KEY, JSON.stringify(s)); } catch { /* storage unavailable: in-memory only */ } }

function Marketplace({ mods = SAMPLE_MODS, currentUser = "You", initialTheme = "k1", theme: controlledTheme, showThemeSwitcher = true, className = "", ...rest }) {
  inject("ms-mk-css", MARKET_CSS);
  const { Badge, Button, Tabs, ThemeSwitcher } = __ds_ns;
  const persisted = useMemo(loadState, []);
  const [ownTheme, setTheme] = useState(persisted?.theme || initialTheme);
  // A host page can drive the skin (controlled); otherwise the built-in switcher owns it.
  const theme = controlledTheme || ownTheme;
  const [query, setQuery] = useState("");
  const [cat, setCat] = useState("All");
  const [game, setGame] = useState("all");
  const [sort, setSort] = useState("installs");
  const [layout, setLayout] = useState(persisted?.layout || "grid");
  const [selectedId, setSelectedId] = useState(mods[0]?.id || null);
  const [tab, setTab] = useState("overview");
  const [installs, setInstalls] = useState(persisted?.installs || {});      // id → {state, progress}
  const [starred, setStarred] = useState(persisted?.starred || {});         // id → true
  const [ownReviews, setOwnReviews] = useState(persisted?.ownReviews || {}); // id → review
  const [helpful, setHelpful] = useState(persisted?.helpful || {});         // reviewId → true
  const [composing, setComposing] = useState(false);
  const [starFilter, setStarFilter] = useState(null);
  const [reviewSort, setReviewSort] = useState("helpful");
  const [toast, setToast] = useState(null);
  const timers = useRef({});

  useEffect(() => { saveState({ theme: ownTheme, layout, installs, starred, ownReviews, helpful }); }, [ownTheme, layout, installs, starred, ownReviews, helpful]);
  useEffect(() => () => Object.values(timers.current).forEach(clearInterval), []);
  const notify = useCallback((msg) => { setToast(msg); clearTimeout(timers.current.toast); timers.current.toast = setTimeout(() => setToast(null), 2200); }, []);

  const allReviews = useCallback((m) => { const own = ownReviews[m.id]; return own ? [own, ...m.reviews] : m.reviews; }, [ownReviews]);
  const ratingOf = useCallback((m) => { const rs = allReviews(m); const n = rs.length; const avg = n ? rs.reduce((a, r) => a + r.rating, 0) / n : 0; const dist = [0, 0, 0, 0, 0]; rs.forEach((r) => { dist[r.rating - 1]++; }); return { avg, n, dist }; }, [allReviews]);

  const visible = useMemo(() => {
    const q = query.trim().toLowerCase();
    let list = mods.filter((m) => (cat === "All" || m.category === cat) && (game === "all" || m.games.includes(game)) && (!q || `${m.name} ${m.author} ${m.summary} ${m.category}`.toLowerCase().includes(q)));
    const key = { installs: (m) => m.installs, rating: (m) => ratingOf(m).avg * 1000 + ratingOf(m).n, stars: (m) => m.stars + (starred[m.id] ? 1 : 0), updated: (m) => Date.parse(m.updated), name: (m) => m.name }[sort];
    list = list.slice().sort((a, b) => { const ka = key(a), kb = key(b); return typeof ka === "string" ? ka.localeCompare(kb) : kb - ka; });
    return list;
  }, [mods, query, cat, game, sort, ratingOf, starred]);
  const counts = useMemo(() => Object.fromEntries(CATEGORIES.map((c) => [c, mods.filter((m) => (c === "All" || m.category === c) && (game === "all" || m.games.includes(game))).length])), [mods, game]);
  const selected = mods.find((m) => m.id === selectedId) || null;

  const startInstall = (m) => {
    if (installs[m.id]?.state === "installing") return;
    setInstalls((s) => ({ ...s, [m.id]: { state: "installing", progress: 0 } }));
    notify(`Installing ${m.name}…`);
    clearInterval(timers.current[m.id]);
    timers.current[m.id] = setInterval(() => {
      setInstalls((s) => { const cur = s[m.id] || { progress: 0 }; const p = Math.min(100, (cur.progress || 0) + 7 + Math.random() * 9); if (p >= 100) { clearInterval(timers.current[m.id]); notify(`${m.name} installed ✓`); return { ...s, [m.id]: { state: "installed", progress: 100 } }; } return { ...s, [m.id]: { state: "installing", progress: p } }; });
    }, 160);
  };
  const uninstall = (m) => { setInstalls((s) => { const n = { ...s }; delete n[m.id]; return n; }); notify(`${m.name} removed (snapshot kept)`); };
  const toggleStar = (m) => { setStarred((s) => { const n = { ...s }; if (n[m.id]) delete n[m.id]; else n[m.id] = true; return n; }); };
  const toggleHelpful = (r) => { setHelpful((s) => { const n = { ...s }; if (n[r.id]) delete n[r.id]; else n[r.id] = true; return n; }); };
  const submitReview = (m, data) => { const prev = ownReviews[m.id]; setOwnReviews((s) => ({ ...s, [m.id]: { id: `own-${m.id}`, author: currentUser, ...data, date: new Date().toISOString(), helpful: prev?.helpful || 0, edited: !!prev } })); setComposing(false); notify(prev ? "Review updated" : "Thanks! Your review is live."); };
  const deleteReview = (m) => { setOwnReviews((s) => { const n = { ...s }; delete n[m.id]; return n; }); notify("Review deleted"); };

  const seg = (opts, value, onChange, label) => h("div", { className: "ms-mk__seg", role: "group", "aria-label": label }, opts.map(([v, l]) => h("button", { key: v, type: "button", "aria-pressed": value === v, onClick: () => onChange(v) }, l)));

  /* ---- detail panel ---- */
  let detail;
  if (!selected) detail = h("div", { className: "ms-mk__dselect" }, "Select a mod to see details, ratings and reviews.");
  else {
    const m = selected; const { avg, n, dist } = ratingOf(m); const inst = installs[m.id] || { state: "idle", progress: 0 };
    const own = ownReviews[m.id];
    let reviews = allReviews(m).map((r) => ({ ...r, helpful: (r.helpful || 0) + (helpful[r.id] ? 1 : 0), helpfulByMe: !!helpful[r.id] }));
    if (starFilter) reviews = reviews.filter((r) => r.rating === starFilter);
    reviews.sort((a, b) => reviewSort === "helpful" ? b.helpful - a.helpful : reviewSort === "newest" ? Date.parse(b.date) - Date.parse(a.date) : reviewSort === "high" ? b.rating - a.rating : a.rating - b.rating);
    if (own) reviews = [reviews.find((r) => r.id === own.id), ...reviews.filter((r) => r.id !== own.id)].filter(Boolean);
    const tabItems = [{ id: "overview", label: "Overview" }, { id: "reviews", label: `Reviews (${n})` }, { id: "versions", label: "Versions" }, { id: "deps", label: "Dependencies" }];
    const tabBar = Tabs ? h(Tabs, { tabs: tabItems, value: tab, onChange: setTab, style: { padding: "0 18px" } }) : h("div", { className: "ms-mk__seg", style: { margin: "0 18px" } }, tabItems.map((t) => h("button", { key: t.id, type: "button", "aria-pressed": tab === t.id, onClick: () => setTab(t.id) }, t.label)));
    const body = tab === "overview" ? [
      h("p", { key: "d", style: { margin: 0, whiteSpace: "pre-wrap" } }, m.description),
      h("div", { key: "b" }, h("h4", null, "Classification"), Badge ? h("div", { style: { display: "flex", gap: 6, flexWrap: "wrap", marginTop: 6 } }, h(Badge, { variant: "category" }, m.category), h(Badge, { variant: "tier" }, m.tier), m.games.map((g) => h(Badge, { key: g, variant: "neutral" }, g))) : null),
      h("div", { key: "k" }, h("h4", null, "Details"), h("dl", { className: "ms-mk__kv", style: { marginTop: 6 } }, h("dt", null, "Version"), h("dd", null, m.version), h("dt", null, "Updated"), h("dd", null, fmtDate(m.updated)), h("dt", null, "Installs"), h("dd", null, m.installs.toLocaleString()), h("dt", null, "Source"), h("dd", null, m.links.map(([l, href]) => h("a", { key: l, className: "ms-mk__link", href, target: "_blank", rel: "noreferrer" }, l))))),
      h("div", { key: "r" }, h("h4", null, "Ratings"), h("div", { style: { marginTop: 8 } }, h(RatingSummary, { average: avg, distribution: dist, activeStar: starFilter, onSelectStar: (s) => { setStarFilter(s); setTab("reviews"); } }))),
    ] : tab === "reviews" ? [
      h(RatingSummary, { key: "s", average: avg, distribution: dist, activeStar: starFilter, onSelectStar: setStarFilter }),
      composing || (own && composing) ? h(ReviewComposer, { key: "c", initial: composing === "edit" ? own : undefined, version: m.version, onSubmit: (d) => submitReview(m, d), onCancel: () => setComposing(false) })
        : h("div", { key: "t", className: "ms-mk__rtools" },
            !own ? (Button ? h(Button, { variant: "primary", size: "sm", icon: "✏️", onClick: () => setComposing(true) }, "Write a Review") : h("button", { type: "button", onClick: () => setComposing(true) }, "Write a Review")) : h("span", null, "You reviewed this mod."),
            h("span", { style: { marginLeft: "auto" } }, "Sort "),
            h("select", { value: reviewSort, onChange: (e) => setReviewSort(e.target.value), "aria-label": "Sort reviews" }, h("option", { value: "helpful" }, "Most helpful"), h("option", { value: "newest" }, "Newest"), h("option", { value: "high" }, "Highest rated"), h("option", { value: "low" }, "Lowest rated")),
            starFilter ? h("button", { type: "button", className: "ms-review__act", onClick: () => setStarFilter(null) }, `✕ ${starFilter}★ only`) : null
          ),
      h("div", { key: "l", className: "ms-mk__reviews" }, reviews.length ? reviews.map((r) => h(ReviewCard, { key: r.id, review: r, own: own && r.id === own.id, onHelpful: own && r.id === own.id ? undefined : toggleHelpful, onEdit: () => setComposing("edit"), onDelete: () => deleteReview(m) })) : h("div", { className: "ms-mk__empty" }, starFilter ? `No ${starFilter}-star reviews yet.` : "No reviews yet. Be the first to write one.")),
    ] : tab === "versions" ? [
      h("div", { key: "v", className: "ms-mk__vers" }, m.versions.map(([v, d, note]) => h("div", { key: v, className: "ms-mk__ver" }, h("code", null, `v${v}`), h("span", null, note), h("span", { style: { color: "var(--text-secondary)" } }, fmtDate(d))))),
    ] : [
      h("div", { key: "dep" }, h("h4", null, "Requires"), m.dependencies.length ? h("div", { style: { display: "flex", gap: 6, flexWrap: "wrap", marginTop: 6 } }, m.dependencies.map((d) => Badge ? h(Badge, { key: d, variant: "dependency", dot: true }, `Requires ${d}`) : d)) : h("p", { style: { margin: "6px 0 0", color: "var(--text-secondary)" } }, "No dependencies.")),
      h("div", { key: "con" }, h("h4", null, "Conflicts"), m.conflicts.length ? h("div", { style: { display: "flex", gap: 6, flexWrap: "wrap", marginTop: 6 } }, m.conflicts.map((c) => Badge ? h(Badge, { key: c, variant: "restriction", dot: true }, `Conflicts ${c}`) : c)) : h("p", { style: { margin: "6px 0 0", color: "var(--text-secondary)" } }, "No known conflicts.")),
      h("p", { key: "n", style: { margin: 0, color: "var(--text-secondary)", fontSize: "var(--text-xs)" } }, "ModSync resolves install order from these relationships and creates a recovery snapshot before writing."),
    ];
    detail = h(React.Fragment, null,
      h("div", { className: "ms-mk__dhead" },
        h("div", { className: "ms-mk__dicon", "aria-hidden": true }, m.icon),
        h("div", null,
          h("div", { className: "ms-mk__dname" }, m.name),
          h("div", { className: "ms-mk__dby" }, `by ${m.author}${m.authorVerified ? " ✓" : ""} · v${m.version}`),
          h("div", { className: "ms-mk__dstats" },
            h("span", null, h(StarRating, { value: avg, count: n, size: "sm", showValue: true })),
            h("span", null, "⬇ ", h("b", null, fmtCount(m.installs)), " installs"),
            h("span", null, "★ ", h("b", null, fmtCount(m.stars + (starred[m.id] ? 1 : 0))), " stars")
          )
        )
      ),
      h("div", { className: "ms-mk__dact" },
        h(InstallButton, { state: inst.state, progress: inst.progress, onInstall: () => startInstall(m), onUpdate: () => startInstall(m), onUninstall: () => uninstall(m) }),
        Button ? h(Button, { variant: starred[m.id] ? "primary" : "secondary", size: "sm", icon: starred[m.id] ? "★" : "☆", onClick: () => toggleStar(m) }, starred[m.id] ? "Starred" : "Star") : null,
        Button ? h(Button, { variant: "secondary", size: "sm", icon: "✏️", onClick: () => { setTab("reviews"); setComposing(own ? "edit" : true); } }, own ? "Edit Review" : "Review") : null
      ),
      tabBar,
      h("div", { className: "ms-mk__dbody" }, body)
    );
  }

  const themeClass = theme === "tsl" ? "theme-tsl" : theme === "light" ? "theme-light" : "";
  return h("div", { className: cx("ms-mk", themeClass, className), "data-theme": theme, style: { position: "relative" }, ...rest },
    h("div", { className: "ms-mk__top" },
      h("div", { className: "ms-mk__brand" }, "🌘 ModSync ", h("small", null, "Marketplace")),
      h("label", { className: "ms-mk__search" }, h("span", { "aria-hidden": true }, "🔎"), h("input", { type: "search", placeholder: "Search mods, authors, categories…", value: query, onChange: (e) => setQuery(e.target.value), "aria-label": "Search mods" })),
      h("div", { className: "ms-mk__topr" },
        seg([["grid", "▦ Grid"], ["list", "☰ List"]], layout, setLayout, "Layout"),
        showThemeSwitcher && !controlledTheme && ThemeSwitcher ? h(ThemeSwitcher, { value: theme, onChange: setTheme }) : null
      )
    ),
    h("aside", { className: "ms-mk__side" },
      h("div", null, h("div", { className: "ms-mk__h" }, "Game"), seg([["all", "All"], ["K1", "K1"], ["TSL", "TSL"]], game, setGame, "Game")),
      h("div", null, h("div", { className: "ms-mk__h" }, "Categories"), h("nav", { className: "ms-mk__nav" }, CATEGORIES.map((c) => h("button", { key: c, type: "button", className: cx("ms-mk__navb", cat === c && "ms-mk__navb--on"), "aria-pressed": cat === c, onClick: () => setCat(c) }, c, h("small", null, counts[c]))))),
      h("div", null, h("div", { className: "ms-mk__h" }, "Your library"), h("nav", { className: "ms-mk__nav" },
        h("div", { className: "ms-mk__navb", style: { cursor: "default" } }, "Installed", h("small", null, Object.values(installs).filter((i) => i.state === "installed").length)),
        h("div", { className: "ms-mk__navb", style: { cursor: "default" } }, "Starred", h("small", null, Object.keys(starred).length)),
        h("div", { className: "ms-mk__navb", style: { cursor: "default" } }, "Reviewed", h("small", null, Object.keys(ownReviews).length))
      ))
    ),
    h("main", { className: "ms-mk__main" },
      h("div", { className: "ms-mk__bar" }, h("span", null, `${visible.length} mod${visible.length === 1 ? "" : "s"}`), h("span", { style: { marginLeft: "auto" } }, "Sort by "), h("select", { value: sort, onChange: (e) => setSort(e.target.value), "aria-label": "Sort mods" }, h("option", { value: "installs" }, "Most installed"), h("option", { value: "rating" }, "Highest rated"), h("option", { value: "stars" }, "Most starred"), h("option", { value: "updated" }, "Recently updated"), h("option", { value: "name" }, "Name"))),
      visible.length ? h("div", { className: layout === "grid" ? "ms-mk__grid" : "ms-mk__list" }, visible.map((m) => { const r = ratingOf(m); const inst = installs[m.id] || {}; return h(ModCard, { key: m.id, mod: { ...m, rating: r.avg, ratingCount: r.n }, layout, selected: m.id === selectedId, installState: inst.state || "idle", installProgress: inst.progress || 0, starred: !!starred[m.id], onSelect: () => { setSelectedId(m.id); setTab("overview"); setComposing(false); setStarFilter(null); }, onInstall: () => startInstall(m), onUpdate: () => startInstall(m), onToggleStar: () => toggleStar(m) }); })) : h("div", { className: "ms-mk__empty" }, "No mods match. Try another category or clear the search.")
    ),
    h("section", { className: "ms-mk__detail", "aria-label": "Mod details" }, detail),
    toast ? h("div", { className: "ms-mk__toast", role: "status" }, toast) : null
  );
}

__ds_ns.StarRating = StarRating;
__ds_ns.RatingSummary = RatingSummary;
__ds_ns.ReviewCard = ReviewCard;
__ds_ns.ReviewComposer = ReviewComposer;
__ds_ns.InstallButton = InstallButton;
__ds_ns.ModCard = ModCard;
__ds_ns.Marketplace = Marketplace;
__ds_ns.MARKETPLACE_SAMPLE = SAMPLE_MODS;
})();
