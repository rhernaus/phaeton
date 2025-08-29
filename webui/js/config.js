// Config form rendering and actions
window.currentConfig = null;
window.currentSchema = null;

window.createInput = function (fieldKey, def, value, path) {
  const wrap = document.createElement('div');
  wrap.className = 'form-field';
  const id = `${path.join('__')}`;
  const label = document.createElement('label');
  label.htmlFor = id; label.textContent = def.title || fieldKey; wrap.appendChild(label);
  let input = null; const error = document.createElement('div'); error.className = 'error'; error.style.display = 'none';
  switch (def.type) {
    case 'string': input = document.createElement('input'); input.type = 'text'; if (def.format === 'ipv4') { input.placeholder = 'e.g. 192.168.1.100'; input.pattern = '^(?:[0-9]{1,3}\\.){3}[0-9]{1,3}$'; } input.value = value ?? ''; break;
    case 'integer': input = document.createElement('input'); input.type = 'number'; input.step = '1'; if (def.min !== null && def.min !== undefined) input.min = String(def.min); if (def.max !== null && def.max !== undefined) input.max = String(def.max); input.value = value !== null && value !== undefined ? String(value) : ''; break;
    case 'number': input = document.createElement('input'); input.type = 'number'; input.step = def.step !== null && def.step !== undefined ? String(def.step) : 'any'; if (def.min !== null && def.min !== undefined) input.min = String(def.min); if (def.max !== null && def.max !== undefined) input.max = String(def.max); input.value = value !== null && value !== undefined ? String(value) : ''; break;
    case 'boolean': input = document.createElement('input'); input.type = 'checkbox'; input.checked = !!value; break;
    case 'enum': input = document.createElement('select'); (def.values || []).forEach(opt => { const o = document.createElement('option'); o.value = String(opt); o.textContent = String(opt); if (String(value) === String(opt)) o.selected = true; input.appendChild(o); }); break;
    case 'time': input = document.createElement('input'); input.type = 'time'; input.value = value || '00:00'; break;
    case 'array': { const container = document.createElement('div'); container.className = 'days'; const days = ['Mon','Tue','Wed','Thu','Fri','Sat','Sun']; const set = new Set((value || []).map(n => Number(n))); days.forEach((name, idx) => { const chip = document.createElement('div'); chip.className = 'day-chip' + (set.has(idx) ? ' active' : ''); chip.textContent = name; chip.addEventListener('click', () => { chip.classList.toggle('active'); }); container.appendChild(chip); }); input = container; break; }
    default: input = document.createElement('input'); input.type = 'text'; input.value = value ?? '';
  }
  input.id = id; wrap.appendChild(input); wrap.appendChild(error); return wrap;
};

window.getValueFromInput = function (input, def) {
  if (def.type === 'boolean') return input.checked;
  if (def.type === 'integer') return input.value === '' ? null : parseInt(input.value, 10);
  if (def.type === 'number') return input.value === '' ? null : parseFloat(input.value);
  if (def.type === 'array' && def.ui === 'days') { const arr = []; Array.from(input.querySelectorAll('.day-chip')).forEach((chip, idx) => { if (chip.classList.contains('active')) arr.push(idx); }); return arr; }
  return input.value;
};

// Default value helpers and stable list indexing
window.defaultValueForField = function (def) {
  switch (def.type) {
    case 'boolean': return false;
    case 'array': return [];
    case 'time': return '00:00';
    case 'enum': return (def.values && def.values.length) ? String(def.values[0]) : '';
    case 'integer':
    case 'number': return null;
    case 'string': return '';
    default: return '';
  }
};

window.defaultsForFields = function (fields) {
  const obj = {};
  Object.keys(fields || {}).forEach(k => { obj[k] = defaultValueForField(fields[k]); });
  return obj;
};

window.nextListIndex = function (listWrap) {
  const next = Number(listWrap.dataset.nextIndex || '0');
  listWrap.dataset.nextIndex = String(next + 1);
  return next;
};

window.validateField = function (input, def) {
  const val = getValueFromInput(input, def); let error = '';
  if ((def.type === 'integer' || def.type === 'number') && val !== null && val !== undefined) {
    if (def.min !== null && def.min !== undefined && val < def.min) error = `Must be ≥ ${def.min}`;
    if (!error && def.max !== null && def.max !== undefined && val > def.max) error = `Must be ≤ ${def.max}`;
  }
  if (def.type === 'string' && def.format === 'ipv4' && val) {
    const re = /^(?:[0-9]{1,3}\.){3}[0-9]{1,3}$/; if (!re.test(val)) error = 'Invalid IPv4 address';
  }
  const errEl = input.parentElement.querySelector('.error'); if (error) { errEl.textContent = error; errEl.style.display = ''; return { ok: false, value: val }; }
  errEl.textContent = ''; errEl.style.display = 'none'; return { ok: true, value: val };
};

window.buildSection = function (container, key, sectionDef, cfg) {
  const section = document.createElement('div'); section.className = 'section' + (sectionDef.advanced ? ' advanced' : ''); section.id = `section_${key}`;
  const header = document.createElement('div'); header.className = 'section-header';
  const title = document.createElement('div'); title.className = 'section-title'; title.textContent = sectionDef.title || key; header.appendChild(title);
  const chevron = document.createElement('span'); chevron.className = 'section-chevron'; chevron.textContent = '▸'; header.appendChild(chevron);
  const body = document.createElement('div'); body.className = 'section-body'; header.addEventListener('click', () => { if (section.classList.contains('open')) { section.classList.remove('open'); chevron.textContent = '▸'; } else { section.classList.add('open'); chevron.textContent = '▾'; } });
  section.appendChild(header); section.appendChild(body);
  if (sectionDef.type === 'object') {
    const fields = sectionDef.fields || {};
    Object.keys(fields).forEach(fkey => {
      const def = fields[fkey]; const value = cfg && cfg[fkey];
      if (def.type === 'list') {
        const listWrap = document.createElement('div'); listWrap.className = 'list-items'; listWrap.dataset.listKey = fkey; listWrap.dataset.nextIndex = '0'; const items = Array.isArray(value) ? value : [];
        function addItem(itemCfg = {}) {
          const itemEl = document.createElement('div'); itemEl.className = 'list-item'; const itemBody = document.createElement('div'); const fields2 = (def.item && def.item.fields) || {}; const defaults = defaultsForFields(fields2); const initCfg = Object.assign({}, defaults, itemCfg); const idx = nextListIndex(listWrap);
          Object.keys(fields2).forEach(f2 => { const def2 = fields2[f2]; const val2 = initCfg[f2]; const fieldEl2 = createInput(f2, def2, val2, [ key, fkey, String(idx), f2 ]); itemBody.appendChild(fieldEl2); });
          const actions = document.createElement('div'); actions.className = 'list-actions'; const removeBtn = document.createElement('button'); removeBtn.className = 'remove-btn'; removeBtn.textContent = 'Remove'; removeBtn.addEventListener('click', () => { listWrap.removeChild(itemEl); }); actions.appendChild(removeBtn); itemEl.appendChild(itemBody); itemEl.appendChild(actions); listWrap.appendChild(itemEl);
        }
        items.forEach(it => addItem(it)); const add = document.createElement('button'); add.className = 'add-btn'; add.textContent = fkey === 'items' && key === 'schedule' ? 'Add schedule' : 'Add item'; add.addEventListener('click', () => addItem({})); body.appendChild(listWrap); body.appendChild(add);
      } else {
        const path = [key, fkey]; const fieldEl = createInput(fkey, def, value, path); fieldEl.dataset.path = JSON.stringify(path); body.appendChild(fieldEl);
      }
    });
    // Special-case: when schedule.mode exists, toggle visibility of items list depending on selection
    if (key === 'schedule' && fields.mode) {
      const modeSelect = body.querySelector('#schedule__mode');
      const updateVisibility = () => {
        const mode = modeSelect && modeSelect.value;
        // Keep schedule items visible for management in all modes.
        // Add an informational notice when Tibber mode is selected.
        const tibberNoticeId = 'tibber_notice';
        let notice = body.querySelector('#' + tibberNoticeId);
        if (mode === 'tibber') {
          if (!notice) {
            notice = document.createElement('div');
            notice.id = tibberNoticeId;
            notice.style.fontSize = '13px';
            notice.style.color = '#475569';
            notice.textContent = 'Tibber mode active: time-based schedules below are editable but inactive until you switch mode to "time".';
            body.appendChild(notice);
          }
        } else if (notice) { notice.remove(); }
      };
      if (modeSelect) modeSelect.addEventListener('change', updateVisibility);
      setTimeout(updateVisibility, 0);
    }
  } else if (sectionDef.type === 'list') {
    const listWrap = document.createElement('div'); listWrap.className = 'list-items'; listWrap.dataset.nextIndex = '0'; const items = (cfg && cfg.items) || [];
    function addItem(itemCfg = {}) {
      const itemEl = document.createElement('div'); itemEl.className = 'list-item'; const itemBody = document.createElement('div'); const fields = sectionDef.item.fields || {}; const defaults = defaultsForFields(fields); const initCfg = Object.assign({}, defaults, itemCfg); const idx = nextListIndex(listWrap);
      Object.keys(fields).forEach(fkey => { const def = fields[fkey]; const value = initCfg[fkey]; const fieldEl = createInput(fkey, def, value, [ key, 'items', String(idx), fkey, ]); itemBody.appendChild(fieldEl); });
      const actions = document.createElement('div'); actions.className = 'list-actions'; const removeBtn = document.createElement('button'); removeBtn.className = 'remove-btn'; removeBtn.textContent = 'Remove'; removeBtn.addEventListener('click', () => { listWrap.removeChild(itemEl); }); actions.appendChild(removeBtn); itemEl.appendChild(itemBody); itemEl.appendChild(actions); listWrap.appendChild(itemEl);
    }
    items.forEach(it => addItem(it)); const add = document.createElement('button'); add.className = 'add-btn'; add.textContent = key === 'vehicles' ? 'Add vehicle' : 'Add schedule'; add.addEventListener('click', () => addItem(key === 'vehicles' ? { provider: 'tesla', poll_interval_seconds: 60 } : {})); body.appendChild(listWrap); body.appendChild(add);
  } else if (sectionDef.type === 'integer' || sectionDef.type === 'string' || sectionDef.type === 'boolean') { const fieldEl = createInput(key, sectionDef, cfg, [key]); fieldEl.dataset.path = JSON.stringify([key]); body.appendChild(fieldEl); }
  container.appendChild(section);
};

window.buildForm = function (schema, cfg) {
  const root = $('config_form'); if (!root) return; root.innerHTML = '';
  const sections = schema.sections || {}; const nav = $('config_nav'); if (nav) nav.innerHTML = '';
  Object.keys(sections).forEach(key => { buildSection(root, key, sections[key], cfg[key]); });
  // Wire expand/collapse controls
  const btnExpand = $('expand_all'); const btnCollapse = $('collapse_all');
  const setOpen = (open) => {
    const sectionsEls = Array.from(root.getElementsByClassName('section'));
    sectionsEls.forEach(s => { const chevron = s.querySelector('.section-chevron'); if (open) { s.classList.add('open'); if (chevron) chevron.textContent = '▾'; } else { s.classList.remove('open'); if (chevron) chevron.textContent = '▸'; } });
  };
  if (btnExpand) btnExpand.onclick = () => setOpen(true);
  if (btnCollapse) btnCollapse.onclick = () => setOpen(false);

  // Wire search filter
  const search = $('config_search');
  if (search) {
    const normalize = (s) => (s || '').toString().toLowerCase();
    const highlight = (el, term) => {
      // remove existing
      el.querySelectorAll('.highlight').forEach(h => { const parent = h.parentNode; parent.replaceChild(document.createTextNode(h.textContent), h); parent.normalize && parent.normalize(); });
      if (!term) return;
      const label = el.querySelector('label'); if (!label) return;
      const text = label.textContent; const idx = text.toLowerCase().indexOf(term.toLowerCase());
      if (idx >= 0) {
        const before = document.createTextNode(text.slice(0, idx));
        const mark = document.createElement('span'); mark.className = 'highlight'; mark.textContent = text.slice(idx, idx + term.length);
        const after = document.createTextNode(text.slice(idx + term.length));
        label.textContent = ''; label.appendChild(before); label.appendChild(mark); label.appendChild(after);
      }
    };
    search.addEventListener('input', () => {
      const term = normalize(search.value);
      const sectionEls = Array.from(root.getElementsByClassName('section'));
      sectionEls.forEach(section => {
        let anyMatch = false;
        const fields = Array.from(section.querySelectorAll('.form-field'));
        fields.forEach(field => {
          const label = field.querySelector('label'); const input = field.querySelector('input, select, .days');
          const text = [label && label.textContent, input && (input.value || input.placeholder || '')].filter(Boolean).join(' ');
          const match = term.length === 0 || normalize(text).includes(term);
          field.style.display = match ? '' : 'none'; field.classList.toggle('match', !!term && match); highlight(field, term);
          if (match) anyMatch = true;
        });
        const chevron = section.querySelector('.section-chevron');
        if (term && anyMatch) { section.classList.add('open'); if (chevron) chevron.textContent = '▾'; }
        else if (term && !anyMatch) { section.classList.remove('open'); if (chevron) chevron.textContent = '▸'; }
        else { /* no search */ }
      });
    });
  }
};

window.openConfigSection = function (key) {
  const root = $('config_form'); const section = document.getElementById(`section_${key}`); if (!root || !section) return; Array.from(root.getElementsByClassName('section')).forEach(s => { s.classList.remove('open'); const ch = s.querySelector('.section-chevron'); if (ch) ch.textContent = '▸'; }); section.classList.add('open'); const chev = section.querySelector('.section-chevron'); if (chev) chev.textContent = '▾'; section.scrollIntoView({ behavior: 'smooth', block: 'start' });
};

window.collectConfig = function (schema) {
  const cfg = JSON.parse(JSON.stringify(currentConfig)); const sections = schema.sections || {}; const root = $('config_form');
  Object.keys(sections).forEach(key => {
    const def = sections[key];
    if (def.type === 'object') {
      const fields = def.fields || {}; cfg[key] = cfg[key] || {};
      Object.keys(fields).forEach(fkey => {
        const fieldDef = fields[fkey];
        if (fieldDef.type === 'list') {
          // Collect list items array
          const sectionEl = document.getElementById(`section_${key}`);
          const listWrap = sectionEl ? sectionEl.querySelector('.section-body .list-items[data-list-key="' + fkey + '"]') : null;
          const arr = [];
          if (listWrap) {
            Array.from(listWrap.children).forEach(itemEl => {
              const fields2 = (fieldDef.item && fieldDef.item.fields) || {};
              const item = {};
              Object.keys(fields2).forEach(f2 => {
                const input = itemEl.querySelector(`[id$="__${f2}"]`) || itemEl.querySelector('.days');
                const { ok, value } = validateField(input, fields2[f2]); if (!ok) { throw new Error(`${key}.${fkey}.${f2}: invalid`); }
                item[f2] = value;
              });
              arr.push(item);
            });
          }
          cfg[key][fkey] = arr;
        } else {
          const fieldEl = Array.from(root.querySelectorAll('.form-field')).find(el => { const path = el.dataset.path && JSON.parse(el.dataset.path); return path && path[0] === key && path[1] === fkey; }); if (!fieldEl) return; const input = fieldEl.querySelector('input, select, .days'); const { ok, value } = validateField(input, fieldDef); if (!ok) { throw new Error(`${key}.${fkey}: invalid`); } cfg[key][fkey] = value;
        }
      });
    } else if (def.type === 'list') {
      cfg[key] = cfg[key] || {}; cfg[key].items = []; const sectionEl = document.getElementById(`section_${key}`); const listWrap = sectionEl ? sectionEl.querySelector('.section-body .list-items') : null; if (listWrap) { Array.from(listWrap.children).forEach(itemEl => { const fields = def.item.fields || {}; const item = {}; Object.keys(fields).forEach(fkey => { const input = itemEl.querySelector(`[id$="__${fkey}"]`) || itemEl.querySelector('.days'); const { ok, value } = validateField(input, fields[fkey]); if (!ok) { throw new Error(`${key}.items.${fkey}: invalid`); } item[fkey] = value; }); cfg[key].items.push(item); }); }
    } else if (def.type === 'integer' || def.type === 'string') {
      const sectionEl = document.getElementById(`section_${key}`); const fieldEl = sectionEl ? sectionEl.querySelector('.section-body .form-field') : null; if (fieldEl) { const input = fieldEl.querySelector('input'); const { ok, value } = validateField(input, def); if (!ok) { throw new Error(`${key}: invalid`); } cfg[key] = value; }
    }
  });
  return cfg;
};

window.saveConfig = async function () {
  const statusEl = $('config_status'); const saveBtn = $('save_config');
  try {
    if (statusEl) { statusEl.textContent = 'Saving...'; statusEl.style.background = 'rgba(59, 130, 246, 0.1)'; statusEl.style.color = '#3b82f6'; }
    if (saveBtn) { saveBtn.style.opacity = '0.7'; saveBtn.style.pointerEvents = 'none'; }
    const payload = collectConfig(currentSchema); const resp = await postJSON('/api/config', payload, 'PUT');
    if (resp.ok) {
      if (statusEl) { statusEl.textContent = '✅ Configuration saved successfully!'; statusEl.style.background = 'rgba(16, 185, 129, 0.1)'; statusEl.style.color = '#10b981'; setTimeout(() => { statusEl.textContent = ''; statusEl.style.background = ''; statusEl.style.color = ''; }, 3000); }
      currentConfig = payload; window.fetchStatus();
    } else if (statusEl) { statusEl.textContent = `❌ Error: ${resp.error || 'Validation failed'}`; statusEl.style.background = 'rgba(239, 68, 68, 0.1)'; statusEl.style.color = '#ef4444'; }
  } catch (e) {
    if (statusEl) { statusEl.textContent = `❌ ${e.message || 'Invalid configuration'}`; statusEl.style.background = 'rgba(239, 68, 68, 0.1)'; statusEl.style.color = '#ef4444'; }
  } finally {
    if (saveBtn) { saveBtn.style.opacity = ''; saveBtn.style.pointerEvents = ''; }
  }
};

window.initConfigForm = async function () {
  try { [currentSchema, currentConfig] = await Promise.all([ getJSON('/api/config/schema'), getJSON('/api/config'), ]); buildForm(currentSchema, currentConfig); } catch (e) { const statusEl = $('config_status'); if (statusEl) { statusEl.textContent = '❌ Failed to load configuration UI'; statusEl.style.background = 'rgba(239, 68, 68, 0.1)'; statusEl.style.color = '#ef4444'; }
  }
};


