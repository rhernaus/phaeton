// Control event wiring
(function initControls() {
  const manual = document.getElementById('mode_manual');
  const auto = document.getElementById('mode_auto');
  const sched = document.getElementById('mode_sched');
  const chargeBtn = document.getElementById('charge_btn');
  const slider = document.getElementById('current_slider');

  function scheduleModeRevert(expectedMode) {
    window.pendingModeTimer = setTimeout(() => {
      if (window.lastStatusData && Number(window.lastStatusData.mode) !== expectedMode) {
        setModeUI(Number(window.lastStatusData.mode || 0));
      }
      window.pendingMode = null;
      window.modeDirtyUntil = 0;
    }, 3000);
  }

  async function setMode(modeValue) {
    window.modeDirtyUntil = Date.now() + 3000;
    window.pendingMode = modeValue;
    if (window.pendingModeTimer) { clearTimeout(window.pendingModeTimer); window.pendingModeTimer = null; }
    setModeUI(modeValue);
    scheduleModeRevert(modeValue);
    try {
      const resp = await postJSON('/api/mode', { mode: modeValue });
      if (!resp || resp.ok === false) {
        setModeUI(Number(window.lastStatusData?.mode || 0));
        window.pendingMode = null; window.modeDirtyUntil = 0;
        if (window.pendingModeTimer) { clearTimeout(window.pendingModeTimer); window.pendingModeTimer = null; }
      } else {
        if (window.pendingModeTimer) { clearTimeout(window.pendingModeTimer); window.pendingModeTimer = null; }
      }
    } catch (_) {
      setModeUI(Number(window.lastStatusData?.mode || 0));
      window.pendingMode = null; window.modeDirtyUntil = 0;
      if (window.pendingModeTimer) { clearTimeout(window.pendingModeTimer); window.pendingModeTimer = null; }
    }
  }

  if (manual) manual.addEventListener('click', () => setMode(0));
  if (auto) auto.addEventListener('click', () => setMode(1));
  if (sched) sched.addEventListener('click', () => setMode(2));

  if (chargeBtn) {
    addButtonFeedback(chargeBtn);
    chargeBtn.addEventListener('click', async () => {
      const isEnabled = !chargeBtn.classList.contains('start');
      chargeBtn.style.opacity = '0.7';
      chargeBtn.style.pointerEvents = 'none';
      try {
        setChargeUI(!isEnabled);
        await postJSON('/api/startstop', { value: !isEnabled ? 1 : 0 });
        chargeBtn.style.transform = 'scale(1.05)';
        setTimeout(() => { chargeBtn.style.transform = ''; }, 200);
      } catch (_) {
        setChargeUI(isEnabled);
      } finally {
        chargeBtn.style.opacity = '';
        chargeBtn.style.pointerEvents = '';
      }
    });
  }

  let currentChangeTimer = null;
  if (slider) {
    addButtonFeedback(slider);
    slider.addEventListener('input', () => {
      window.currentDirtyUntil = Date.now() + 2000;
      document.getElementById('current_display').textContent = `${Math.round(slider.value)} A`;
      slider.setAttribute('aria-valuenow', String(Math.round(slider.value)));
      if (currentChangeTimer) clearTimeout(currentChangeTimer);
      currentChangeTimer = setTimeout(async () => {
        const amps = parseFloat(slider.value);
        await postJSON('/api/set_current', { amps });
      }, 400);
    });
    slider.addEventListener('pointerdown', () => { window.currentDirtyUntil = Date.now() + 5000; });
    slider.addEventListener('pointerup', () => { window.currentDirtyUntil = Date.now() + 1500; });
  }
})();


