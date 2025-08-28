// Bootstrap only
resizeChartCanvas();
fetchStatus();
initConfigForm();
initUX();
setInterval(() => { resizeChartCanvas(); fetchStatus(); }, 2000);

// Fetch app version once for header display
(async () => {
  try {
    const res = await fetch('/api/update/status');
    const s = await res.json();
    if (s && s.current_version && document.getElementById('app_ver')) {
      document.getElementById('app_ver').textContent = `v${s.current_version}`;
    }
  } catch (_) {}
})();

// Session timer soft update while charging
setInterval(() => {
  const sessionTimeEl = document.getElementById('session_time');
  if (sessionTimeEl && window.lastStatusData && window.lastStatusData.status === 2) {
    const sess = window.lastStatusData.session || {};
    if (typeof sess.charging_time_sec === 'number' && sess.charging_time_sec >= 0) {
      const base = sess.charging_time_sec;
      const duration = Math.floor(base + (Date.now() / 1000 - (window.lastStatusData._last_update_ts || Date.now() / 1000)));
      const h = Math.floor(duration / 3600), m = Math.floor((duration % 3600) / 60), s = duration % 60;
      sessionTimeEl.textContent = `${h.toString().padStart(2,'0')}:${m.toString().padStart(2,'0')}:${s.toString().padStart(2,'0')}`;
    } else if (sess && sess.start_ts) {
      const startTime = new Date(sess.start_ts).getTime();
      const duration = Math.floor((Date.now() - startTime) / 1000);
      const h = Math.floor(duration / 3600), m = Math.floor((duration % 3600) / 60), s = duration % 60;
      sessionTimeEl.textContent = `${h.toString().padStart(2,'0')}:${m.toString().padStart(2,'0')}:${s.toString().padStart(2,'0')}`;
    }
  }
}, 1000);


