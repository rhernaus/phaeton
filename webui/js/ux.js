// UX, views, updates, logs
window.modeDirtyUntil = 0;
window.currentDirtyUntil = 0;
window.pendingMode = null;
window.pendingModeTimer = null;
window.isConfigOpen = false;

window.setModeUI = function (mode) {
  if (Date.now() < modeDirtyUntil) return;
  ['mode_manual','mode_auto','mode_sched'].forEach(id => { const btn = $(id); btn.classList.remove('active'); btn.setAttribute('aria-pressed','false'); });
  if (mode === 0) { $('mode_manual').classList.add('active'); $('mode_manual').setAttribute('aria-pressed','true'); }
  else if (mode === 1) { $('mode_auto').classList.add('active'); $('mode_auto').setAttribute('aria-pressed','true'); }
  else if (mode === 2) { $('mode_sched').classList.add('active'); $('mode_sched').setAttribute('aria-pressed','true'); }
};

window.setChargeUI = function (enabled) {
  const btn = $('charge_btn'); const icon = btn.querySelector('.btn-icon'); const text = btn.querySelector('span:not(.btn-icon)');
  if (enabled) { if (icon) icon.textContent = '⏹️'; if (text) text.textContent = 'Stop'; btn.classList.remove('start'); btn.classList.add('stop'); btn.setAttribute('aria-pressed','true'); btn.setAttribute('aria-label','Stop charging'); }
  else { if (icon) icon.textContent = '▶️'; if (text) text.textContent = 'Start'; btn.classList.remove('stop'); btn.classList.add('start'); btn.setAttribute('aria-pressed','false'); btn.setAttribute('aria-label','Start charging'); }
};

window.setCurrentUI = function (displayAmps, stationMax) {
  if (Date.now() < currentDirtyUntil) return; const slider = $('current_slider'); $('current_display').textContent = `${Math.round(displayAmps)} A`; if (slider && stationMax > 0) { const max = Math.min(stationMax, 25); slider.max = String(max); slider.setAttribute('aria-valuemax', String(max)); }
};

window.setConnectionState = function (ok) {
  const dot = $('conn_dot'); const text = $('conn_text'); if (!dot || !text) return; dot.style.transition = 'all 0.3s ease'; text.style.transition = 'all 0.3s ease'; if (ok) { dot.style.background = '#22c55e'; dot.style.boxShadow = '0 0 0 3px rgba(34,197,94,0.2)'; text.textContent = 'Online'; text.style.color = '#22c55e'; } else { dot.style.background = '#ef4444'; dot.style.boxShadow = '0 0 0 3px rgba(239,68,68,0.2)'; text.textContent = 'Offline'; text.style.color = '#ef4444'; }
};

window.showError = function (msg) { const el = document.getElementById('error_banner'); if (!el) return; if (msg) { el.textContent = msg; el.style.display = ''; } else { el.textContent = ''; el.style.display = 'none'; } };

window.switchView = function (viewName) {
  const dashboardContent = $('dashboard_content'); const configContent = $('config_content'); const updatesContent = $('updates_content'); const logsContent = $('logs_content'); const dashboardBtn = $('dashboard_view'); const configBtn = $('config_view'); const updatesBtn = $('updates_view'); const logsBtn = $('logs_view');
  if (!dashboardContent || !configContent || !dashboardBtn || !configBtn || !updatesContent || !updatesBtn || !logsContent || !logsBtn) return;
  function setButtons(d,c,u,l) { dashboardBtn.classList.toggle('active', d); configBtn.classList.toggle('active', c); updatesBtn.classList.toggle('active', u); logsBtn.classList.toggle('active', l); dashboardBtn.setAttribute('aria-pressed', String(d)); configBtn.setAttribute('aria-pressed', String(c)); updatesBtn.setAttribute('aria-pressed', String(u)); logsBtn.setAttribute('aria-pressed', String(l)); }
  function show(el) { el.style.display = 'block'; el.style.opacity = '1'; }
  function hide(el) { el.style.opacity = '0'; setTimeout(() => { el.style.display = 'none'; }, 150); }
  if (viewName === 'dashboard') { hide(configContent); hide(updatesContent); hide(logsContent); show(dashboardContent); setButtons(true,false,false,false); isConfigOpen = false; }
  else if (viewName === 'config') { hide(dashboardContent); hide(updatesContent); hide(logsContent); show(configContent); setButtons(false,true,false,false); isConfigOpen = true; if (currentSchema && currentConfig) { buildForm(currentSchema, currentConfig); } else { initConfigForm(); } }
  else if (viewName === 'updates') { hide(dashboardContent); hide(configContent); hide(logsContent); show(updatesContent); setButtons(false,false,true,false); (async () => { try { await fetch('/api/update/check', { method: 'POST' }); } catch (e) {} try { const res = await fetch('/api/update/status'); const s = await res.json(); const statEl = $('updates_status'); if (statEl && s) { const parts = []; if (s.current_version) parts.push(`current: ${s.current_version}`); if (s.latest_version) parts.push(`latest: ${s.latest_version}`); if (typeof s.update_available === 'boolean') parts.push(s.update_available ? 'update available' : 'up to date'); statEl.textContent = parts.join(' | '); } } catch (e) {} try { if (typeof window.loadReleases === 'function') window.loadReleases(); } catch (e) {} })(); }
  else if (viewName === 'logs') { hide(dashboardContent); hide(configContent); hide(updatesContent); show(logsContent); setButtons(false,false,false,true); ensureLogsStream(); }
};

window.initUX = function () {
  const dashboardBtn = $('dashboard_view'); const configBtn = $('config_view'); const updatesBtn = $('updates_view'); const logsBtn = $('logs_view'); const menuToggle = $('menu_toggle'); const menuDropdown = $('menu_dropdown');
  if (dashboardBtn) { dashboardBtn.addEventListener('click', () => { switchView('dashboard'); if (menuDropdown) { menuDropdown.style.display = 'none'; if (menuToggle) menuToggle.setAttribute('aria-expanded','false'); } }); addButtonFeedback(dashboardBtn); }
  if (configBtn) { configBtn.addEventListener('click', () => { switchView('config'); if (menuDropdown) { menuDropdown.style.display = 'none'; if (menuToggle) menuToggle.setAttribute('aria-expanded','false'); } }); addButtonFeedback(configBtn); }
  if (updatesBtn) { updatesBtn.addEventListener('click', () => { switchView('updates'); if (menuDropdown) { menuDropdown.style.display = 'none'; if (menuToggle) menuToggle.setAttribute('aria-expanded','false'); } }); addButtonFeedback(updatesBtn); }
  if (logsBtn) { logsBtn.addEventListener('click', () => { switchView('logs'); if (menuDropdown) { menuDropdown.style.display = 'none'; if (menuToggle) menuToggle.setAttribute('aria-expanded','false'); } }); addButtonFeedback(logsBtn); }
  if (menuToggle && menuDropdown) {
    menuToggle.addEventListener('click', () => { const isOpen = menuDropdown.style.display !== 'none'; menuDropdown.style.display = isOpen ? 'none' : ''; menuToggle.setAttribute('aria-expanded', String(!isOpen)); });
    document.addEventListener('click', e => { const target = e.target; if (!menuDropdown || !menuToggle) return; if (target !== menuDropdown && target !== menuToggle && !menuDropdown.contains(target) && !menuToggle.contains(target)) { if (menuDropdown.style.display !== 'none') { menuDropdown.style.display = 'none'; menuToggle.setAttribute('aria-expanded','false'); } } });
  }
  const saveBtn = $('save_config'); if (saveBtn) { saveBtn.addEventListener('click', saveConfig); addButtonFeedback(saveBtn); }
  const btnCheck = $('btn_check_updates'); const btnApply = $('btn_apply_updates'); const statEl = $('updates_status'); const releaseSelect = $('release_select');
  async function refreshUpdateStatus() { try { const res = await fetch('/api/update/status'); const s = await res.json(); if (statEl) { const parts = []; if (s.current_version) parts.push(`current: ${s.current_version}`); if (s.latest_version) parts.push(`latest: ${s.latest_version}`); if (typeof s.update_available === 'boolean') parts.push(s.update_available ? 'update available' : 'up to date'); statEl.textContent = parts.join(' | '); } } catch (e) { if (statEl) statEl.textContent = 'Failed to load update status'; } }
  async function loadReleases() {
    if (!releaseSelect) return;
    try {
      const r = await fetch('/api/update/releases');
      const arr = await r.json();
      releaseSelect.innerHTML = '';
      if (!Array.isArray(arr) || arr.length === 0) {
        const opt = document.createElement('option');
        opt.value = '';
        opt.textContent = 'No releases found';
        releaseSelect.appendChild(opt);
        releaseSelect.disabled = true;
        if ($('btn_switch_release')) $('btn_switch_release').disabled = true;
        const rn = $('release_notes'); if (rn) rn.style.display = 'none';
        return;
      }
      releaseSelect.disabled = false;
      if ($('btn_switch_release')) $('btn_switch_release').disabled = false;
      arr.forEach(rel => {
        const opt = document.createElement('option');
        opt.value = String(rel.tag || rel.tag_name || '');
        const label = rel.name ? `${rel.tag} - ${rel.name}` : String(rel.tag);
        opt.textContent = label;
        opt.dataset.body = rel.body ? String(rel.body) : '';
        releaseSelect.appendChild(opt);
      });
      // Show notes for selected release if present
      const sel = releaseSelect.selectedOptions && releaseSelect.selectedOptions[0];
      const body = sel && sel.dataset.body ? sel.dataset.body : '';
      const box = $('release_notes');
      const pre = $('release_notes_pre');
      if (box && pre) {
        if (body) { pre.textContent = body; box.style.display = ''; }
        else { pre.textContent = ''; box.style.display = 'none'; }
      }
      releaseSelect.addEventListener('change', () => {
        const s = releaseSelect.selectedOptions && releaseSelect.selectedOptions[0];
        const b = s && s.dataset.body ? s.dataset.body : '';
        const bx = $('release_notes');
        const pr = $('release_notes_pre');
        if (bx && pr) {
          if (b) { pr.textContent = b; bx.style.display = ''; }
          else { pr.textContent = ''; bx.style.display = 'none'; }
        }
      });
    } catch (e) {
      releaseSelect.innerHTML = '';
      const opt = document.createElement('option');
      opt.value = '';
      opt.textContent = 'Failed to load releases';
      releaseSelect.appendChild(opt);
      releaseSelect.disabled = true;
      if ($('btn_switch_release')) $('btn_switch_release').disabled = true;
      const rn = $('release_notes'); if (rn) rn.style.display = 'none';
    }
  }
  window.loadReleases = loadReleases; if (btnCheck) { btnCheck.addEventListener('click', async () => { if (statEl) statEl.textContent = 'Checking for updates...'; try { const res = await fetch('/api/update/check', { method: 'POST' }); const s = await res.json(); const payload = s.status || s; const parts = []; if (payload.current_version) parts.push(`current: ${payload.current_version}`); if (payload.latest_version) parts.push(`latest: ${payload.latest_version}`); if (typeof payload.update_available === 'boolean') parts.push(payload.update_available ? 'update available' : 'up to date'); if (statEl) statEl.textContent = parts.join(' | '); } catch (e) { if (statEl) statEl.textContent = 'Check failed'; } }); }
  if (btnApply) { btnApply.addEventListener('click', async () => { if (statEl) statEl.textContent = 'Applying latest release and restarting...'; try { const res = await fetch('/api/update/apply', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({}), }); const s = await res.json(); if (s && s.ok) { if (statEl) statEl.textContent = 'Restarting... please wait; reloading without cache...'; setTimeout(() => { try { const url = new URL(window.location.href); url.searchParams.set('reload', String(Date.now())); window.location.replace(url.toString()); } catch (e) { window.location.reload(); } }, 7000); } else { if (statEl) statEl.textContent = `Update failed: ${s && s.error ? s.error : 'unknown error'}`; } } catch (e) { if (statEl) statEl.textContent = 'Update failed'; } }); }
  if ($('btn_switch_release') && releaseSelect) { $('btn_switch_release').addEventListener('click', async () => { const version = releaseSelect.value; if (!version) return; if (statEl) statEl.textContent = `Switching to ${version}...`; try { const res = await fetch('/api/update/apply', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ version }), }); const s = await res.json(); if (s && s.ok) { if (statEl) statEl.textContent = `Restarting into ${version}...`; setTimeout(() => { try { const url = new URL(window.location.href); url.searchParams.set('reload', String(Date.now())); window.location.replace(url.toString()); } catch (e) { window.location.reload(); } }, 7000); } else if (statEl) { statEl.textContent = `Switch failed: ${s && s.error ? s.error : 'unknown error'}`; } } catch (e) { if (statEl) statEl.textContent = 'Switch failed'; } }); }
  refreshUpdateStatus(); if (releaseSelect) loadReleases();
  // Quick settings cards removed from HTML; no-op
  switchView('dashboard');
};

// Logs stream
window.logsEventSource = null; window.logsPaused = false;
window.appendLogLine = function (line) { const el = $('logs_pre'); if (!el) return; el.textContent += (el.textContent ? '\n' : '') + line; if (!logsPaused) el.scrollTop = el.scrollHeight; };
window.ensureLogsStream = function () {
  if (logsEventSource) return; const pauseChk = $('logs_pause'); if (pauseChk) pauseChk.addEventListener('change', () => { logsPaused = !!pauseChk.checked; }); const clearBtn = $('btn_clear_logs'); if (clearBtn) clearBtn.addEventListener('click', () => { const el = $('logs_pre'); if (el) el.textContent = ''; });
  try { logsEventSource = new EventSource('/api/logs/stream'); logsEventSource.onmessage = ev => { if (typeof ev.data === 'string') appendLogLine(ev.data); }; logsEventSource.onerror = () => { if (logsEventSource) { logsEventSource.close(); logsEventSource = null; } setTimeout(ensureLogsStream, 2000); }; } catch (e) {}
};


