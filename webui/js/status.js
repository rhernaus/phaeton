// Status polling and UI updates
window.fetchStatus = async function () {
  try {
    const res = await fetch('/api/status');
    const s = await res.json();
    s._last_update_ts = Date.now() / 1000;
    setConnectionState(true);
    showError('');
    window.lastStatusData = s;
    setTextIfExists('product', s.product_name || '');
    setTextIfExists('serial', s.serial ? `SN ${s.serial}` : '');
    setTextIfExists('firmware', s.firmware ? `FW ${s.firmware}` : '');
    if (pendingMode !== null && Number(s.mode ?? 0) === Number(pendingMode)) {
      pendingMode = null;
      modeDirtyUntil = 0;
      if (pendingModeTimer) { clearTimeout(pendingModeTimer); pendingModeTimer = null; }
    }
    setModeUI(Number(s.mode ?? 0));
    setChargeUI(Number(s.start_stop ?? 1) === 1);
    const mode = Number(s.mode ?? 0);
    const setpoint = Number(s.set_current ?? 6.0);
    let displayCurrent = setpoint;
    if (mode === 1 || mode === 2) { displayCurrent = Number(s.applied_current ?? setpoint); }
    const stationMax = Number(s.station_max_current ?? 0);
    setCurrentUI(displayCurrent, stationMax);
    const slider = $('current_slider');
    if (slider && Date.now() >= currentDirtyUntil) {
      const val = mode === 1 ? displayCurrent : setpoint;
      slider.value = String(val);
      slider.setAttribute('aria-valuenow', String(Math.round(val)));
    }
    setTextIfExists('di', s.device_instance ?? '');
    const stName = statusNames[s.status] || '-';
    setTextIfExists('status_text', s.status === 2 ? `Charging ${Number(s.active_phases) === 1 ? '1P' : '3P'}` : stName);
    const phasesToggle = $('phases_toggle');
    if (phasesToggle) {
      const mode = Number(s.mode ?? 0);
      // Disable in Auto mode (auto switching) and reflect current phases
      phasesToggle.disabled = mode === 1;
      phasesToggle.checked = Number(s.active_phases || 0) >= 3;
    }
    const p = Number(s.ac_power || 0);
    const powerEl = $('hero_power_w');
    if (powerEl) {
      const currentPower = parseInt(powerEl.textContent) || 0;
      const newPower = Math.round(p);
      if (Math.abs(newPower - currentPower) > 10) {
        powerEl.style.transform = 'scale(1.1)';
        powerEl.style.transition = 'all 0.3s ease';
        setTimeout(() => { powerEl.style.transform = ''; }, 300);
      }
      powerEl.textContent = newPower >= 1000 ? (newPower / 1000).toFixed(2) : newPower;
    }
    const unitEl = $('hero_power_unit'); if (unitEl) { unitEl.textContent = p >= 1000 ? 'kW' : 'W'; }
    if ($('session_time')) {
      const sess = s.session || {};
      if (typeof sess.charging_time_sec === 'number' && sess.charging_time_sec >= 0) {
        const duration = Math.floor(sess.charging_time_sec);
        const hours = Math.floor(duration / 3600);
        const minutes = Math.floor((duration % 3600) / 60);
        const seconds = duration % 60;
        $('session_time').textContent = `${hours.toString().padStart(2,'0')}:${minutes.toString().padStart(2,'0')}:${seconds.toString().padStart(2,'0')}`;
      } else if (sess && sess.start_ts) {
        const startTime = new Date(sess.start_ts).getTime();
        const endTime = sess.end_ts ? new Date(sess.end_ts).getTime() : Date.now();
        const duration = Math.floor((endTime - startTime) / 1000);
        const hours = Math.floor(duration / 3600);
        const minutes = Math.floor((duration % 3600) / 60);
        const seconds = duration % 60;
        $('session_time').textContent = `${hours.toString().padStart(2,'0')}:${minutes.toString().padStart(2,'0')}:${seconds.toString().padStart(2,'0')}`;
      } else {
        $('session_time').textContent = '00:00:00';
      }
    }
    if ($('session_energy')) {
      const energy = (s.session && typeof s.session.energy_delivered_kwh === 'number') ? s.session.energy_delivered_kwh : 0;
      $('session_energy').textContent = Number(energy).toFixed(2);
    }
    if ($('session_cost')) {
      let cost = s.session && s.session.cost;
      if (cost === null || cost === undefined) {
        const energy = (s.session && typeof s.session.energy_delivered_kwh === 'number') ? s.session.energy_delivered_kwh : 0;
        const rate = s.energy_rate ?? 0.25; cost = energy * rate;
      }
      const currency = s.pricing_currency || '€';
      $('session_cost').textContent = `${currency}${Number(cost).toFixed(2)}`;
    }
    if ($('total_energy')) { const totalEnergy = s.total_energy_kwh ?? 0; $('total_energy').textContent = Number(totalEnergy).toFixed(2); }
    addHistoryPoint(s);
    if (!isConfigOpen && currentSchema && currentConfig) { /* no-op heavy rebuild avoided */ }
  } catch (e) {
    setConnectionState(false);
    showError('Failed to fetch status. Retrying…');
    // eslint-disable-next-line no-console
    console.error('status error', e);
  }
};


