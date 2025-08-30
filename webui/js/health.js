// Health/status view logic
window.refreshHealth = async function () {
  try {
    const [healthResp, metrics] = await Promise.all([
      fetch('/api/health'),
      (async () => { try { const r = await fetch('/api/metrics'); return await r.json(); } catch (_) { return null; } })(),
    ]);
    const webOk = healthResp && healthResp.ok;
    const setVal = (id, txt) => { const el = document.getElementById(id); if (el) el.textContent = txt; };
    setVal('st_web', webOk ? 'OK' : 'ERROR');
    if (metrics) {
      setVal('st_driver', metrics.driver_state || '-');
      const conn = typeof metrics.modbus_connected === 'boolean' ? (metrics.modbus_connected ? 'Connected' : 'Disconnected') : 'Unknown';
      setVal('st_modbus', conn);
      const ageMs = typeof metrics.age_ms === 'number' ? metrics.age_ms : -1;
      setVal('st_age', window.formatAge(ageMs));
      setVal('st_total_polls', String(metrics.total_polls ?? '-'));
      setVal('st_overruns', String(metrics.overrun_count ?? '-'));
      const pi = metrics.poll_interval_ms; setVal('st_interval', typeof pi === 'number' ? `${pi} ms` : '-');
      // Feed step timings history if available
      if (metrics && metrics.poll_steps_ms) {
        window.addPollStepHistory(metrics.poll_steps_ms);
      }
    } else {
      setVal('st_driver', '-');
      setVal('st_modbus', 'Unknown');
      setVal('st_age', '-');
      setVal('st_total_polls', '-');
      setVal('st_overruns', '-');
      setVal('st_interval', '-');
    }
  } catch (_) {
    const setVal = (id, txt) => { const el = document.getElementById(id); if (el) el.textContent = txt; };
    setVal('st_web', 'ERROR');
  }
};

// Auto-refresh status view when visible
(function setupHealthAutoRefresh() {
  let timer = null;
  function tick() {
    const el = document.getElementById('status_content');
    if (el && el.style.display !== 'none') {
      if (typeof window.refreshHealth === 'function') window.refreshHealth();
    }
  }
  function start() {
    if (timer) return; timer = setInterval(tick, 3000);
  }
  function stop() { if (!timer) return; clearInterval(timer); timer = null; }
  document.addEventListener('visibilitychange', () => { if (document.hidden) stop(); else start(); });
  start();
})();


