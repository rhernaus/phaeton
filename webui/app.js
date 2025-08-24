const $ = id => document.getElementById(id);

function setTextIfExists(id, text) {
  const el = $(id);
  if (el) {
    el.textContent = text;
  }
}

const statusNames = {
  0: 'Disconnected',
  1: 'Connected',
  2: 'Charging',
  3: 'Charged',
  4: 'Wait sun',
  6: 'Wait start',
  7: 'Low SOC',
};

let currentConfig = null;
let currentSchema = null;

// History series
const chartHistory = {
  points: [], // {t, current, allowed, station}
  windowSec: 300,
  maxBufferSec: 21600, // keep up to 6h for smooth window changes
  hoverT: null,
};

function addHistoryPoint(s) {
  const t = Date.now() / 1000;
  const current = Number(s.ac_current || 0);
  let allowed = Number(s.set_current || 0);
  const station = Number(s.station_max_current || 0);
  const mode = Number(s.mode || 0);
  if (mode === 1) {
    // AUTO
    allowed = Number(s.applied_current ?? allowed);
  } else if (mode === 2) {
    // SCHEDULED
    allowed = Number(s.applied_current ?? allowed);
  }
  chartHistory.points.push({ t, current, allowed, station });
  const cutoff = t - chartHistory.maxBufferSec;
  chartHistory.points = chartHistory.points.filter(p => p.t >= cutoff);
  drawChart();
}

function drawDotOnChart(ctx, x, y, color) {
  ctx.fillStyle = color;
  ctx.beginPath();
  ctx.arc(x, y, 3, 0, Math.PI * 2);
  ctx.fill();
}

function drawChart() {
  const canvas = $('chart');
  if (!canvas) {
    return;
  }
  const ctx = canvas.getContext('2d');
  const dpr = window.devicePixelRatio || 1;
  const W = canvas.width / dpr;
  const H = canvas.height / dpr;
  ctx.clearRect(0, 0, W, H);
  ctx.fillStyle = '#1a2332';
  ctx.fillRect(0, 0, W, H);
  if (chartHistory.points.length < 2) {
    return;
  }
  const tEnd = chartHistory.points[chartHistory.points.length - 1].t;
  const tMinDesired = tEnd - chartHistory.windowSec;
  const visible = chartHistory.points.filter(p => p.t >= tMinDesired);
  if (visible.length < 2) {
    return;
  }
  const tMin = visible[0].t;
  const tMax = visible[visible.length - 1].t;
  const tSpan = Math.max(1, tMax - tMin);
  let vMax = 0;
  visible.forEach(p => {
    vMax = Math.max(vMax, p.current, p.allowed, p.station);
  });
  vMax = Math.max(10, Math.ceil(vMax / 5) * 5);
  function mapX(t) {
    return 40 + ((t - tMin) / tSpan) * (W - 60);
  }
  function mapY(v) {
    return H - 20 - (v / vMax) * (H - 40);
  }
  // Grid (horizontal)
  ctx.strokeStyle = 'rgba(255,255,255,0.1)';
  ctx.lineWidth = 1;
  for (let i = 0; i <= 5; i++) {
    const y = mapY((vMax / 5) * i);
    ctx.beginPath();
    ctx.moveTo(40, y);
    ctx.lineTo(W - 20, y);
    ctx.stroke();
  }
  // Series draw function
  function plot(color, key) {
    ctx.strokeStyle = color;
    ctx.lineWidth = 2;
    ctx.beginPath();
    visible.forEach((p, idx) => {
      const x = mapX(p.t);
      const y = mapY(p[key]);
      if (idx === 0) {
        ctx.moveTo(x, y);
      } else {
        ctx.lineTo(x, y);
      }
    });
    ctx.stroke();
  }
  plot('#22c55e', 'current');
  plot('#f59e0b', 'allowed');
  plot('#ef4444', 'station');
  // Axes
  ctx.strokeStyle = 'rgba(255,255,255,0.2)';
  ctx.lineWidth = 1;
  ctx.beginPath();
  ctx.moveTo(40, 10);
  ctx.lineTo(40, H - 20);
  ctx.lineTo(W - 20, H - 20);
  ctx.stroke();
  ctx.fillStyle = '#8899aa';
  ctx.font = '12px -apple-system, sans-serif';
  ctx.fillText(`${vMax} A`, 4, mapY(vMax) + 4);
  ctx.fillText('0', 20, H - 22);

  // X-axis ticks and labels (HH:MM)
  const numTicks = 5;
  const step = tSpan / numTicks;
  ctx.fillStyle = '#94a3b8';
  for (let i = 0; i <= numTicks; i++) {
    const tTick = tMin + step * i;
    const x = mapX(tTick);
    ctx.strokeStyle = 'rgba(255,255,255,0.15)';
    ctx.beginPath();
    ctx.moveTo(x, H - 20);
    ctx.lineTo(x, H - 16);
    ctx.stroke();
    const d = new Date(tTick * 1000);
    const hh = String(d.getHours()).padStart(2, '0');
    const mm = String(d.getMinutes()).padStart(2, '0');
    const label = `${hh}:${mm}`;
    const textW = ctx.measureText(label).width;
    ctx.fillText(label, Math.min(Math.max(40, x - textW / 2), W - 20 - textW), H - 4);
  }

  // Hover crosshair and tooltip
  const tip = $('chart_tooltip');
  if (chartHistory.hoverT && tip) {
    // Find nearest point in visible range
    let nearest = visible[0];
    let bestDt = Math.abs(chartHistory.hoverT - nearest.t);
    for (let i = 1; i < visible.length; i++) {
      const dt = Math.abs(chartHistory.hoverT - visible[i].t);
      if (dt < bestDt) {
        bestDt = dt;
        nearest = visible[i];
      }
    }
    const x = mapX(nearest.t);
    // Vertical line
    ctx.strokeStyle = 'rgba(148,163,184,0.6)';
    ctx.beginPath();
    ctx.moveTo(x, 10);
    ctx.lineTo(x, H - 20);
    ctx.stroke();
    // Points
    drawDotOnChart(ctx, x, mapY(nearest.current), '#22c55e');
    drawDotOnChart(ctx, x, mapY(nearest.allowed), '#f59e0b');
    drawDotOnChart(ctx, x, mapY(nearest.station), '#ef4444');
    // Tooltip content
    const d = new Date(nearest.t * 1000);
    const hh = String(d.getHours()).padStart(2, '0');
    const mm = String(d.getMinutes()).padStart(2, '0');
    const ss = String(d.getSeconds()).padStart(2, '0');
    tip.innerHTML = `${hh}:${mm}:${ss} — cur ${nearest.current.toFixed(
      1
    )} A · allow ${nearest.allowed.toFixed(1)} A · max ${nearest.station.toFixed(0)} A`;
    // Place tooltip
    const rect = canvas.getBoundingClientRect();
    const parent = canvas.parentElement;
    const parentRect = parent
      ? parent.getBoundingClientRect()
      : { left: 0, top: 0, width: rect.width };
    const canvasCssW = rect.width;
    const scale = canvasCssW / W;
    const cssX = x * scale + (rect.left - parentRect.left);
    const top = rect.top - parentRect.top + 12;
    tip.style.left = `${cssX}px`;
    tip.style.top = `${top}px`;
    tip.style.display = '';
  } else if (tip) {
    tip.style.display = 'none';
  }
}

$('range')?.addEventListener('change', e => {
  chartHistory.windowSec = parseInt(e.target.value, 10) || 300;
  drawChart();
});

// Interaction state
let modeDirtyUntil = 0;
let currentDirtyUntil = 0;
let pendingMode = null;
let pendingModeTimer = null;

function setModeUI(mode) {
  // Only update UI if not recently changed by the user
  if (Date.now() < modeDirtyUntil) {
    return;
  }
  ['mode_manual', 'mode_auto', 'mode_sched'].forEach(id => {
    const btn = $(id);
    btn.classList.remove('active');
    btn.setAttribute('aria-pressed', 'false');
  });

  if (mode === 0) {
    $('mode_manual').classList.add('active');
    $('mode_manual').setAttribute('aria-pressed', 'true');
  } else if (mode === 1) {
    $('mode_auto').classList.add('active');
    $('mode_auto').setAttribute('aria-pressed', 'true');
  } else if (mode === 2) {
    $('mode_sched').classList.add('active');
    $('mode_sched').setAttribute('aria-pressed', 'true');
  }
}

function setChargeUI(enabled) {
  const btn = $('charge_btn');
  const icon = btn.querySelector('.btn-icon');
  const text = btn.querySelector('span:not(.btn-icon)');

  if (enabled) {
    if (icon) {
      icon.textContent = '⏹️';
    }
    if (text) {
      text.textContent = 'Stop';
    }
    btn.classList.remove('start');
    btn.classList.add('stop');
    btn.setAttribute('aria-pressed', 'true');
    btn.setAttribute('aria-label', 'Stop charging');
  } else {
    if (icon) {
      icon.textContent = '▶️';
    }
    if (text) {
      text.textContent = 'Start';
    }
    btn.classList.remove('stop');
    btn.classList.add('start');
    btn.setAttribute('aria-pressed', 'false');
    btn.setAttribute('aria-label', 'Start charging');
  }
}

function setCurrentUI(displayAmps, stationMax) {
  if (Date.now() < currentDirtyUntil) {
    return;
  }
  const slider = $('current_slider');
  $('current_display').textContent = `${Math.round(displayAmps)} A`;
  // Update slider min/max based on station capabilities
  if (slider && stationMax > 0) {
    const max = Math.min(stationMax, 25);
    slider.max = String(max);
    slider.setAttribute('aria-valuemax', String(max));
  }
}

function setConnectionState(ok) {
  const dot = $('conn_dot');
  const text = $('conn_text');
  if (!dot || !text) {
    return;
  }

  // Add transition animation
  dot.style.transition = 'all 0.3s ease';
  text.style.transition = 'all 0.3s ease';

  if (ok) {
    dot.style.background = '#22c55e';
    dot.style.boxShadow = '0 0 0 3px rgba(34,197,94,0.2)';
    text.textContent = 'Online';
    text.style.color = '#22c55e';
  } else {
    dot.style.background = '#ef4444';
    dot.style.boxShadow = '0 0 0 3px rgba(239,68,68,0.2)';
    text.textContent = 'Offline';
    text.style.color = '#ef4444';
  }
}

function showError(msg) {
  const el = document.getElementById('error_banner');
  if (!el) {
    return;
  }
  if (msg) {
    el.textContent = msg;
    el.style.display = '';
  } else {
    el.textContent = '';
    el.style.display = 'none';
  }
}

// Responsive canvas handling
let chartDevicePixelRatio = 0;
function resizeChartCanvas() {
  const canvas = $('chart');
  if (!canvas) {
    return;
  }
  const dpr = window.devicePixelRatio || 1;
  if (chartDevicePixelRatio === dpr && canvas.dataset.sized === '1') {
    return;
  }
  const rect = canvas.getBoundingClientRect();
  const cssWidth = Math.floor(rect.width);
  const cssHeight = Math.floor(rect.height);
  canvas.width = Math.max(320, cssWidth) * dpr;
  canvas.height = Math.max(120, cssHeight) * dpr;
  const ctx = canvas.getContext('2d');
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  canvas.dataset.sized = '1';
  chartDevicePixelRatio = dpr;
  drawChart();
}
window.addEventListener('resize', () => {
  chartDevicePixelRatio = 0;
  resizeChartCanvas();
});

// Enhanced visual feedback for interactions
function addButtonFeedback(button) {
  button.addEventListener('click', function () {
    this.style.transform = 'scale(0.95)';
    setTimeout(() => {
      this.style.transform = '';
    }, 150);
  });
}

// Wire controls with enhanced feedback
$('mode_manual').addEventListener('click', async () => {
  modeDirtyUntil = Date.now() + 3000;
  pendingMode = 0;
  if (pendingModeTimer) {
    clearTimeout(pendingModeTimer);
    pendingModeTimer = null;
  }
  setModeUI(0);
  pendingModeTimer = setTimeout(() => {
    if (window.lastStatusData && Number(window.lastStatusData.mode) !== 0) {
      setModeUI(Number(window.lastStatusData.mode || 0));
    }
    pendingMode = null;
    modeDirtyUntil = 0;
  }, 3000);
  try {
    const resp = await postJSON('/api/mode', { mode: 0 });
    if (!resp || resp.ok === false) {
      // Revert immediately on error
      setModeUI(Number(window.lastStatusData?.mode || 0));
      pendingMode = null;
      modeDirtyUntil = 0;
      if (pendingModeTimer) {
        clearTimeout(pendingModeTimer);
        pendingModeTimer = null;
      }
    } else {
      // Success: avoid auto-revert; wait for /api/status confirmation
      if (pendingModeTimer) {
        clearTimeout(pendingModeTimer);
        pendingModeTimer = null;
      }
    }
  } catch (e) {
    setModeUI(Number(window.lastStatusData?.mode || 0));
    pendingMode = null;
    modeDirtyUntil = 0;
    if (pendingModeTimer) {
      clearTimeout(pendingModeTimer);
      pendingModeTimer = null;
    }
  }
});
$('mode_auto').addEventListener('click', async () => {
  modeDirtyUntil = Date.now() + 3000;
  pendingMode = 1;
  if (pendingModeTimer) {
    clearTimeout(pendingModeTimer);
    pendingModeTimer = null;
  }
  setModeUI(1);
  pendingModeTimer = setTimeout(() => {
    if (window.lastStatusData && Number(window.lastStatusData.mode) !== 1) {
      setModeUI(Number(window.lastStatusData.mode || 0));
    }
    pendingMode = null;
    modeDirtyUntil = 0;
  }, 3000);
  try {
    const resp = await postJSON('/api/mode', { mode: 1 });
    if (!resp || resp.ok === false) {
      setModeUI(Number(window.lastStatusData?.mode || 0));
      pendingMode = null;
      modeDirtyUntil = 0;
      if (pendingModeTimer) {
        clearTimeout(pendingModeTimer);
        pendingModeTimer = null;
      }
    } else {
      if (pendingModeTimer) {
        clearTimeout(pendingModeTimer);
        pendingModeTimer = null;
      }
    }
  } catch (e) {
    setModeUI(Number(window.lastStatusData?.mode || 0));
    pendingMode = null;
    modeDirtyUntil = 0;
    if (pendingModeTimer) {
      clearTimeout(pendingModeTimer);
      pendingModeTimer = null;
    }
  }
});
$('mode_sched').addEventListener('click', async () => {
  modeDirtyUntil = Date.now() + 3000;
  pendingMode = 2;
  if (pendingModeTimer) {
    clearTimeout(pendingModeTimer);
    pendingModeTimer = null;
  }
  setModeUI(2);
  pendingModeTimer = setTimeout(() => {
    if (window.lastStatusData && Number(window.lastStatusData.mode) !== 2) {
      setModeUI(Number(window.lastStatusData.mode || 0));
    }
    pendingMode = null;
    modeDirtyUntil = 0;
  }, 3000);
  try {
    const resp = await postJSON('/api/mode', { mode: 2 });
    if (!resp || resp.ok === false) {
      setModeUI(Number(window.lastStatusData?.mode || 0));
      pendingMode = null;
      modeDirtyUntil = 0;
      if (pendingModeTimer) {
        clearTimeout(pendingModeTimer);
        pendingModeTimer = null;
      }
    } else {
      if (pendingModeTimer) {
        clearTimeout(pendingModeTimer);
        pendingModeTimer = null;
      }
    }
  } catch (e) {
    setModeUI(Number(window.lastStatusData?.mode || 0));
    pendingMode = null;
    modeDirtyUntil = 0;
    if (pendingModeTimer) {
      clearTimeout(pendingModeTimer);
      pendingModeTimer = null;
    }
  }
});

$('charge_btn').addEventListener('click', async () => {
  // Toggle with animation
  const isEnabled = !$('charge_btn').classList.contains('start');
  const btn = $('charge_btn');

  // Add loading state
  btn.style.opacity = '0.7';
  btn.style.pointerEvents = 'none';

  try {
    setChargeUI(!isEnabled);
    await postJSON('/api/startstop', { enabled: !isEnabled });

    // Success animation
    btn.style.transform = 'scale(1.05)';
    setTimeout(() => {
      btn.style.transform = '';
    }, 200);
  } catch (error) {
    // eslint-disable-next-line no-console
    console.error('Failed to toggle charging:', error);
    // Revert UI on error
    setChargeUI(isEnabled);
  } finally {
    btn.style.opacity = '';
    btn.style.pointerEvents = '';
  }
});

// Add feedback to all mode buttons
['mode_manual', 'mode_auto', 'mode_sched'].forEach(id => {
  const btn = $(id);
  if (btn) {
    addButtonFeedback(btn);
  }
});

// Add feedback to charge button
const chargeBtn = $('charge_btn');
if (chargeBtn) {
  addButtonFeedback(chargeBtn);
}

let currentChangeTimer = null;
$('current_slider').addEventListener('input', () => {
  currentDirtyUntil = Date.now() + 2000;
  const slider = $('current_slider');
  $('current_display').textContent = `${Math.round(slider.value)} A`;
  slider.setAttribute('aria-valuenow', String(Math.round(slider.value)));
  if (currentChangeTimer) {
    clearTimeout(currentChangeTimer);
  }
  currentChangeTimer = setTimeout(async () => {
    const amps = parseFloat(slider.value);
    await postJSON('/api/set_current', { amps });
  }, 400);
});

// Extend dirty window while the user is dragging the slider
$('current_slider').addEventListener('pointerdown', () => {
  currentDirtyUntil = Date.now() + 5000;
});
$('current_slider').addEventListener('pointerup', () => {
  currentDirtyUntil = Date.now() + 1500;
});

// Chart hover listeners
(function initChartHover() {
  const canvas = $('chart');
  if (!canvas) {
    return;
  }
  canvas.addEventListener('mousemove', e => {
    const rect = canvas.getBoundingClientRect();
    const cssX = e.clientX - rect.left;
    // Recompute current visible window mapping
    if (chartHistory.points.length < 2) {
      return;
    }
    const tEnd = chartHistory.points[chartHistory.points.length - 1].t;
    const tMinDesired = tEnd - chartHistory.windowSec;
    const visible = chartHistory.points.filter(p => p.t >= tMinDesired);
    if (visible.length < 2) {
      return;
    }
    const tMin = visible[0].t;
    const tMax = visible[visible.length - 1].t;
    const tSpan = Math.max(1, tMax - tMin);
    const rectW = rect.width;
    const x = Math.max(40, Math.min(rectW - 20, cssX));
    const frac = (x - 40) / Math.max(1, rectW - 60);
    chartHistory.hoverT = tMin + frac * tSpan;
    drawChart();
  });
  canvas.addEventListener('mouseleave', () => {
    chartHistory.hoverT = null;
    drawChart();
  });
})();

async function fetchStatus() {
  try {
    const res = await fetch('/api/status');
    const s = await res.json();
    s._last_update_ts = Date.now() / 1000;
    setConnectionState(true);
    showError('');
    window.lastStatusData = s; // Store for session timer
    setTextIfExists('product', s.product_name || '');
    setTextIfExists('serial', s.serial ? `SN ${s.serial}` : '');
    setTextIfExists('firmware', s.firmware ? `FW ${s.firmware}` : '');
    // If server confirmed pending mode, clear pending and let UI reflect server
    if (pendingMode !== null && Number(s.mode ?? 0) === Number(pendingMode)) {
      pendingMode = null;
      modeDirtyUntil = 0;
      if (pendingModeTimer) {
        clearTimeout(pendingModeTimer);
        pendingModeTimer = null;
      }
    }
    setModeUI(Number(s.mode ?? 0));
    setChargeUI(Number(s.start_stop ?? 1) === 1);
    // Determine which current to display based on mode
    const mode = Number(s.mode ?? 0);
    const setpoint = Number(s.set_current ?? 6.0);
    let displayCurrent = setpoint;
    if (mode === 1) {
      // AUTO
      displayCurrent = Number(s.applied_current ?? setpoint);
    } else if (mode === 2) {
      // SCHEDULED
      displayCurrent = Number(s.applied_current ?? setpoint);
    }
    // Update display and slider separately
    const stationMax = Number(s.station_max_current ?? 0);
    setCurrentUI(displayCurrent, stationMax);
    const slider = $('current_slider');
    if (slider && Date.now() >= currentDirtyUntil) {
      if (mode === 1) {
        slider.value = String(displayCurrent);
        slider.setAttribute('aria-valuenow', String(Math.round(displayCurrent)));
      } else {
        slider.value = String(setpoint);
        slider.setAttribute('aria-valuenow', String(Math.round(setpoint)));
      }
    }
    setTextIfExists('di', s.device_instance ?? '');
    const stName = statusNames[s.status] || '-';
    setTextIfExists('status', stName);
    setTextIfExists(
      'status_text',
      s.status === 2 ? `Charging ${Number(s.active_phases) === 1 ? '1P' : '3P'}` : stName
    );
    const p = Number(s.ac_power || 0);

    // Animate power value changes
    const powerEl = $('hero_power_w');
    if (powerEl) {
      const currentPower = parseInt(powerEl.textContent) || 0;
      const newPower = Math.round(p);

      if (Math.abs(newPower - currentPower) > 10) {
        powerEl.style.transform = 'scale(1.1)';
        powerEl.style.transition = 'all 0.3s ease';
        setTimeout(() => {
          powerEl.style.transform = '';
        }, 300);
      }

      // Display power in watts or kW if >= 1000W
      powerEl.textContent = newPower >= 1000 ? (newPower / 1000).toFixed(2) : newPower;
    }
    // Display power with conditional units
    setTextIfExists('active_power', p >= 1000 ? (p / 1000).toFixed(2) : Math.round(p));
    const unitEl = $('hero_power_unit');
    if (unitEl) {
      unitEl.textContent = p >= 1000 ? 'kW' : 'W';
    }

    // Update session info elements with actual data from backend
    if ($('session_time')) {
      // Prefer session-level charging_time_sec (seconds) when provided
      const sess = s.session || {};
      if (typeof sess.charging_time_sec === 'number' && sess.charging_time_sec >= 0) {
        const duration = Math.floor(sess.charging_time_sec);
        const hours = Math.floor(duration / 3600);
        const minutes = Math.floor((duration % 3600) / 60);
        const seconds = duration % 60;
        $('session_time').textContent = `${hours.toString().padStart(2, '0')}:${minutes
          .toString()
          .padStart(2, '0')}:${seconds.toString().padStart(2, '0')}`;
      } else if (sess && sess.start_ts) {
        // Fallback: compute from session start/end timestamps
        const startTime = new Date(sess.start_ts).getTime();
        const endTime = sess.end_ts ? new Date(sess.end_ts).getTime() : Date.now();
        const duration = Math.floor((endTime - startTime) / 1000);
        const hours = Math.floor(duration / 3600);
        const minutes = Math.floor((duration % 3600) / 60);
        const seconds = duration % 60;
        $('session_time').textContent = `${hours.toString().padStart(2, '0')}:${minutes
          .toString()
          .padStart(2, '0')}:${seconds.toString().padStart(2, '0')}`;
      } else {
        $('session_time').textContent = '00:00:00';
      }
    }
    if ($('session_energy')) {
      // Use session-level delivered energy
      const energy = (s.session && typeof s.session.energy_delivered_kwh === 'number')
        ? s.session.energy_delivered_kwh
        : 0;
      $('session_energy').textContent = Number(energy).toFixed(2);
    }
    if ($('session_cost')) {
      // Prefer server-calculated session cost if provided under session
      let cost = s.session && s.session.cost;
      if (cost === null || cost === undefined) {
        const energy = (s.session && typeof s.session.energy_delivered_kwh === 'number')
          ? s.session.energy_delivered_kwh
          : 0;
        // Fallback: flat rate per kWh when hourly breakdown is unavailable
        const rate = s.energy_rate ?? 0.25;
        cost = energy * rate;
      }
      const currency = s.pricing_currency || '€';
      $('session_cost').textContent = `${currency}${Number(cost).toFixed(2)}`;
    }
    if ($('total_energy')) {
      // Use total lifetime energy available from charger
      const totalEnergy = s.total_energy_kwh ?? 0;
      $('total_energy').textContent = Number(totalEnergy).toFixed(2);
    }
    // Update active status indicator
    if ($('active_status')) {
      $('active_status').style.color = s.status === 2 ? '#22c55e' : '#666';
    }
    // Update charging port animation
    const chargingPort = document.querySelector('.charging-port');
    if (chargingPort) {
      chargingPort.style.fill = s.status === 2 ? '#22c55e' : '#666';
    }
    setTextIfExists('ac_current', `${(s.ac_current ?? 0).toFixed(2)} A`);
    setTextIfExists('ac_power', p >= 1000 ? `${(p / 1000).toFixed(2)} kW` : `${Math.round(p)} W`);
    setTextIfExists('energy', `${(s.energy_forward_kwh ?? 0).toFixed(3)} kWh`);
    setTextIfExists(
      'l1',
      `${(s.l1_voltage ?? 0).toFixed(1)} V / ${(s.l1_current ?? 0).toFixed(2)} A / ${Math.round(
        s.l1_power ?? 0
      )} W`
    );
    setTextIfExists(
      'l2',
      `${(s.l2_voltage ?? 0).toFixed(1)} V / ${(s.l2_current ?? 0).toFixed(2)} A / ${Math.round(
        s.l2_power ?? 0
      )} W`
    );
    setTextIfExists(
      'l3',
      `${(s.l3_voltage ?? 0).toFixed(1)} V / ${(s.l3_current ?? 0).toFixed(2)} A / ${Math.round(
        s.l3_power ?? 0
      )} W`
    );
    addHistoryPoint(s);
    // only rebuild form when closed to avoid flicker while editing
    if (!isConfigOpen && currentSchema && currentConfig) {
      // no-op here; form rebuild is heavy and only needed after save
    }
  } catch (e) {
    setConnectionState(false);
    showError('Failed to fetch status. Retrying…');
    // eslint-disable-next-line no-console
    console.error('status error', e);
  }
}

async function getJSON(url) {
  const res = await fetch(url);
  return await res.json();
}

async function postJSON(url, payload, method = 'POST') {
  const res = await fetch(url, {
    method,
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(payload),
  });
  return await res.json();
}

function createInput(fieldKey, def, value, path) {
  const wrap = document.createElement('div');
  wrap.className = 'form-field';
  const id = `${path.join('__')}`;
  let labelText = def.title || fieldKey;
  const label = document.createElement('label');
  label.htmlFor = id;
  label.textContent = labelText;
  wrap.appendChild(label);

  let input = null;
  let error = document.createElement('div');
  error.className = 'error';
  error.style.display = 'none';

  switch (def.type) {
    case 'string': {
      input = document.createElement('input');
      input.type = 'text';
      if (def.format === 'ipv4') {
        input.placeholder = 'e.g. 192.168.1.100';
        input.pattern = '^(?:[0-9]{1,3}\\.){3}[0-9]{1,3}$';
      }
      input.value = value ?? '';
      break;
    }
    case 'integer': {
      input = document.createElement('input');
      input.type = 'number';
      input.step = '1';
      if (def.min !== null && def.min !== undefined) {
        input.min = String(def.min);
      }
      if (def.max !== null && def.max !== undefined) {
        input.max = String(def.max);
      }
      input.value = value !== null && value !== undefined ? String(value) : '';
      break;
    }
    case 'number': {
      input = document.createElement('input');
      input.type = 'number';
      input.step = def.step !== null && def.step !== undefined ? String(def.step) : 'any';
      if (def.min !== null && def.min !== undefined) {
        input.min = String(def.min);
      }
      if (def.max !== null && def.max !== undefined) {
        input.max = String(def.max);
      }
      input.value = value !== null && value !== undefined ? String(value) : '';
      break;
    }
    case 'boolean': {
      input = document.createElement('input');
      input.type = 'checkbox';
      input.checked = !!value;
      break;
    }
    case 'enum': {
      input = document.createElement('select');
      (def.values || []).forEach(opt => {
        const o = document.createElement('option');
        o.value = String(opt);
        o.textContent = String(opt);
        if (String(value) === String(opt)) {
          o.selected = true;
        }
        input.appendChild(o);
      });
      break;
    }
    case 'time': {
      input = document.createElement('input');
      input.type = 'time';
      input.value = value || '00:00';
      break;
    }
    case 'array': {
      // Only special case we support here is days-of-week chips
      const container = document.createElement('div');
      container.className = 'days';
      const days = ['Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat', 'Sun'];
      const set = new Set((value || []).map(n => Number(n)));
      days.forEach((name, idx) => {
        const chip = document.createElement('div');
        chip.className = 'day-chip' + (set.has(idx) ? ' active' : '');
        chip.textContent = name;
        chip.addEventListener('click', () => {
          if (chip.classList.contains('active')) {
            chip.classList.remove('active');
          } else {
            chip.classList.add('active');
          }
        });
        container.appendChild(chip);
      });
      input = container;
      break;
    }
    default: {
      input = document.createElement('input');
      input.type = 'text';
      input.value = value ?? '';
    }
  }
  input.id = id;
  wrap.appendChild(input);
  wrap.appendChild(error);
  return wrap;
}

function getValueFromInput(input, def) {
  if (def.type === 'boolean') {
    return input.checked;
  }
  if (def.type === 'integer') {
    return input.value === '' ? null : parseInt(input.value, 10);
  }
  if (def.type === 'number') {
    return input.value === '' ? null : parseFloat(input.value);
  }
  if (def.type === 'array' && def.ui === 'days') {
    const arr = [];
    Array.from(input.querySelectorAll('.day-chip')).forEach((chip, idx) => {
      if (chip.classList.contains('active')) {
        arr.push(idx);
      }
    });
    return arr;
  }
  return input.value;
}

function validateField(input, def) {
  let val = getValueFromInput(input, def);
  let error = '';
  if ((def.type === 'integer' || def.type === 'number') && val !== null && val !== undefined) {
    if (def.min !== null && def.min !== undefined && val < def.min) {
      error = `Must be ≥ ${def.min}`;
    }
    if (!error && def.max !== null && def.max !== undefined && val > def.max) {
      error = `Must be ≤ ${def.max}`;
    }
  }
  if (def.type === 'string' && def.format === 'ipv4' && val) {
    const re = /^(?:[0-9]{1,3}\.){3}[0-9]{1,3}$/;
    if (!re.test(val)) {
      error = 'Invalid IPv4 address';
    }
  }
  const errEl = input.parentElement.querySelector('.error');
  if (error) {
    errEl.textContent = error;
    errEl.style.display = '';
    return { ok: false, value: val };
  }
  errEl.textContent = '';
  errEl.style.display = 'none';
  return { ok: true, value: val };
}

function buildSection(container, key, sectionDef, cfg) {
  const section = document.createElement('div');
  section.className = 'section' + (sectionDef.advanced ? ' advanced' : '');
  section.id = `section_${key}`;
  const header = document.createElement('div');
  header.className = 'section-header';
  const title = document.createElement('div');
  title.className = 'section-title';
  title.textContent = sectionDef.title || key;
  header.appendChild(title);
  const chevron = document.createElement('span');
  chevron.className = 'section-chevron';
  chevron.textContent = '▸';
  header.appendChild(chevron);
  const body = document.createElement('div');
  body.className = 'section-body';
  header.addEventListener('click', () => {
    if (section.classList.contains('open')) {
      section.classList.remove('open');
      chevron.textContent = '▸';
    } else {
      section.classList.add('open');
      chevron.textContent = '▾';
    }
  });
  section.appendChild(header);
  section.appendChild(body);

  if (sectionDef.type === 'object') {
    const fields = sectionDef.fields || {};
    Object.keys(fields).forEach(fkey => {
      const def = fields[fkey];
      const value = cfg && cfg[fkey];
      const fieldEl = createInput(fkey, def, value, [key, fkey]);
      fieldEl.dataset.path = JSON.stringify([key, fkey]);
      body.appendChild(fieldEl);
    });
  } else if (sectionDef.type === 'list') {
    const listWrap = document.createElement('div');
    listWrap.className = 'list-items';
    const items = (cfg && cfg.items) || [];

    // eslint-disable-next-line no-inner-declarations
    function addItem(itemCfg = {}) {
      const itemEl = document.createElement('div');
      itemEl.className = 'list-item';
      const itemBody = document.createElement('div');
      const fields = sectionDef.item.fields || {};
      Object.keys(fields).forEach(fkey => {
        const def = fields[fkey];
        const value = itemCfg[fkey];
        const fieldEl = createInput(fkey, def, value, [
          key,
          'items',
          String(listWrap.children.length),
          fkey,
        ]);
        itemBody.appendChild(fieldEl);
      });
      // Vehicles-specific: show fields for selected provider only
      if (key === 'vehicles') {
        // helper to toggle provider-specific fields visibility
        const applyVisibility = () => {
          const providerSelect = itemBody.querySelector('select[id$="__provider"]');
          const provider = providerSelect ? String(providerSelect.value || '').toLowerCase() : '';
          Array.from(itemBody.querySelectorAll('.form-field')).forEach(ff => {
            const input = ff.querySelector('input, select, .days');
            if (!input || !input.id) {
              ff.style.display = '';
              return;
            }
            const id = input.id;
            // Always visible basic fields
            if (
              id.endsWith('__provider') ||
              id.endsWith('__name') ||
              id.endsWith('__poll_interval_seconds')
            ) {
              ff.style.display = '';
              return;
            }
            // Provider-specific prefixes
            const last = id.split('__').pop() || '';
            if (last.startsWith('tesla_')) {
              ff.style.display = provider === 'tesla' ? '' : 'none';
              return;
            }
            if (last.startsWith('kia_')) {
              ff.style.display = provider === 'kia' ? '' : 'none';
              return;
            }
            // Default: visible
            ff.style.display = '';
          });
        };
        const provSel = itemBody.querySelector('select[id$="__provider"]');
        if (provSel) {
          provSel.addEventListener('change', applyVisibility);
        }
        // Apply initial visibility
        applyVisibility();
      }
      const actions = document.createElement('div');
      actions.className = 'list-actions';
      const removeBtn = document.createElement('button');
      removeBtn.className = 'remove-btn';
      removeBtn.textContent = 'Remove';
      removeBtn.addEventListener('click', () => {
        listWrap.removeChild(itemEl);
      });
      actions.appendChild(removeBtn);
      itemEl.appendChild(itemBody);
      itemEl.appendChild(actions);
      listWrap.appendChild(itemEl);
    }

    items.forEach(it => addItem(it));
    const add = document.createElement('button');
    add.className = 'add-btn';
    if (key === 'vehicles') {
      add.textContent = 'Add vehicle';
      add.addEventListener('click', () =>
        addItem({ provider: 'tesla', poll_interval_seconds: 60 })
      );
    } else {
      add.textContent = 'Add schedule';
      add.addEventListener('click', () =>
        addItem({ active: false, days: [], start_time: '00:00', end_time: '00:00' })
      );
    }
    body.appendChild(listWrap);
    body.appendChild(add);
  } else if (sectionDef.type === 'integer') {
    const fieldEl = createInput(key, sectionDef, cfg, [key]);
    fieldEl.dataset.path = JSON.stringify([key]);
    body.appendChild(fieldEl);
  } else if (sectionDef.type === 'string') {
    const fieldEl = createInput(key, sectionDef, cfg, [key]);
    fieldEl.dataset.path = JSON.stringify([key]);
    body.appendChild(fieldEl);
  }

  container.appendChild(section);
}

function buildForm(schema, cfg) {
  const root = $('config_form');
  if (!root) {
    return;
  }

  root.innerHTML = '';
  const sections = schema.sections || {};
  const nav = $('config_nav');
  if (nav) {
    nav.innerHTML = '';
  }

  Object.keys(sections).forEach(key => {
    buildSection(root, key, sections[key], cfg[key]);

    // navigation chips removed per UX request
  });
}

function openConfigSection(key) {
  const root = $('config_form');
  const section = document.getElementById(`section_${key}`);
  if (!root || !section) {
    return;
  }
  Array.from(root.getElementsByClassName('section')).forEach(s => {
    s.classList.remove('open');
    const ch = s.querySelector('.section-chevron');
    if (ch) {
      ch.textContent = '▸';
    }
  });
  section.classList.add('open');
  const chev = section.querySelector('.section-chevron');
  if (chev) {
    chev.textContent = '▾';
  }
  section.scrollIntoView({ behavior: 'smooth', block: 'start' });
}

function collectConfig(schema) {
  const cfg = JSON.parse(JSON.stringify(currentConfig));
  const sections = schema.sections || {};
  const root = $('config_form');

  // Handle object sections
  Object.keys(sections).forEach(key => {
    const def = sections[key];
    if (def.type === 'object') {
      const fields = def.fields || {};
      cfg[key] = cfg[key] || {};
      Object.keys(fields).forEach(fkey => {
        const fieldDef = fields[fkey];
        const fieldEl = Array.from(root.querySelectorAll('.form-field')).find(el => {
          const path = el.dataset.path && JSON.parse(el.dataset.path);
          return path && path[0] === key && path[1] === fkey;
        });
        if (!fieldEl) {
          return;
        }
        const input = fieldEl.querySelector('input, select, .days');
        const { ok, value } = validateField(input, fieldDef);
        if (!ok) {
          throw new Error(`${key}.${fkey}: invalid`);
        }
        cfg[key][fkey] = value;
      });
    } else if (def.type === 'list') {
      cfg[key] = cfg[key] || {};
      cfg[key].items = [];
      const sectionEl = document.getElementById(`section_${key}`);
      const listWrap = sectionEl ? sectionEl.querySelector('.section-body .list-items') : null;
      if (listWrap) {
        Array.from(listWrap.children).forEach(itemEl => {
          const fields = def.item.fields || {};
          const item = {};
          Object.keys(fields).forEach(fkey => {
            const input =
              itemEl.querySelector(`[id$="__${fkey}"]`) || itemEl.querySelector('.days');
            const { ok, value } = validateField(input, fields[fkey]);
            if (!ok) {
              throw new Error(`${key}.items.${fkey}: invalid`);
            }
            item[fkey] = value;
          });
          cfg[key].items.push(item);
        });
      }
    } else if (def.type === 'integer' || def.type === 'string') {
      const sectionEl = document.getElementById(`section_${key}`);
      const fieldEl = sectionEl ? sectionEl.querySelector('.section-body .form-field') : null;
      if (fieldEl) {
        const input = fieldEl.querySelector('input');
        const { ok, value } = validateField(input, def);
        if (!ok) {
          throw new Error(`${key}: invalid`);
        }
        cfg[key] = value;
      }
    }
  });
  return cfg;
}

async function saveConfig() {
  const statusEl = $('config_status');
  const saveBtn = $('save_config');

  try {
    if (statusEl) {
      statusEl.textContent = 'Saving...';
      statusEl.style.background = 'rgba(59, 130, 246, 0.1)';
      statusEl.style.color = '#3b82f6';
    }

    if (saveBtn) {
      saveBtn.style.opacity = '0.7';
      saveBtn.style.pointerEvents = 'none';
    }

    const payload = collectConfig(currentSchema);
    const resp = await postJSON('/api/config', payload, 'PUT');

    if (resp.ok) {
      if (statusEl) {
        statusEl.textContent = '✅ Configuration saved successfully!';
        statusEl.style.background = 'rgba(16, 185, 129, 0.1)';
        statusEl.style.color = '#10b981';
        setTimeout(() => {
          statusEl.textContent = '';
          statusEl.style.background = '';
          statusEl.style.color = '';
        }, 3000);
      }
      currentConfig = payload;
      fetchStatus();
    } else {
      if (statusEl) {
        statusEl.textContent = `❌ Error: ${resp.error || 'Validation failed'}`;
        statusEl.style.background = 'rgba(239, 68, 68, 0.1)';
        statusEl.style.color = '#ef4444';
      }
    }
  } catch (e) {
    if (statusEl) {
      statusEl.textContent = `❌ ${e.message || 'Invalid configuration'}`;
      statusEl.style.background = 'rgba(239, 68, 68, 0.1)';
      statusEl.style.color = '#ef4444';
    }
  } finally {
    if (saveBtn) {
      saveBtn.style.opacity = '';
      saveBtn.style.pointerEvents = '';
    }
  }
}

async function initConfigForm() {
  try {
    [currentSchema, currentConfig] = await Promise.all([
      getJSON('/api/config/schema'),
      getJSON('/api/config'),
    ]);
    buildForm(currentSchema, currentConfig);
  } catch (e) {
    const statusEl = $('config_status');
    if (statusEl) {
      statusEl.textContent = '❌ Failed to load configuration UI';
      statusEl.style.background = 'rgba(239, 68, 68, 0.1)';
      statusEl.style.color = '#ef4444';
    }
    // eslint-disable-next-line no-console
    console.error('Failed to initialize config form:', e);
  }
}

// Enhanced view management
let isConfigOpen = false;

function switchView(viewName) {
  const dashboardContent = $('dashboard_content');
  const configContent = $('config_content');
  const updatesContent = $('updates_content');
  const logsContent = $('logs_content');
  const dashboardBtn = $('dashboard_view');
  const configBtn = $('config_view');
  const updatesBtn = $('updates_view');
  const logsBtn = $('logs_view');

  if (
    !dashboardContent ||
    !configContent ||
    !dashboardBtn ||
    !configBtn ||
    !updatesContent ||
    !updatesBtn ||
    !logsContent ||
    !logsBtn
  ) {
    return;
  }

  // Handle view switching with smooth animation
  if (viewName === 'dashboard') {
    configContent.style.opacity = '0';
    updatesContent.style.opacity = '0';
    logsContent.style.opacity = '0';
    setTimeout(() => {
      configContent.style.display = 'none';
      updatesContent.style.display = 'none';
      logsContent.style.display = 'none';
      dashboardContent.style.display = 'block';
      dashboardContent.style.opacity = '1';
    }, 150);

    // Update button states
    dashboardBtn.classList.add('active');
    configBtn.classList.remove('active');
    updatesBtn.classList.remove('active');
    logsBtn.classList.remove('active');
    dashboardBtn.setAttribute('aria-pressed', 'true');
    configBtn.setAttribute('aria-pressed', 'false');
    updatesBtn.setAttribute('aria-pressed', 'false');
    logsBtn.setAttribute('aria-pressed', 'false');

    isConfigOpen = false;
  } else if (viewName === 'config') {
    dashboardContent.style.opacity = '0';
    setTimeout(() => {
      dashboardContent.style.display = 'none';
      updatesContent.style.display = 'none';
      logsContent.style.display = 'none';
      configContent.style.display = 'block';
      configContent.style.opacity = '1';
    }, 150);

    // Update button states
    configBtn.classList.add('active');
    dashboardBtn.classList.remove('active');
    updatesBtn.classList.remove('active');
    logsBtn.classList.remove('active');
    configBtn.setAttribute('aria-pressed', 'true');
    dashboardBtn.setAttribute('aria-pressed', 'false');
    updatesBtn.setAttribute('aria-pressed', 'false');
    logsBtn.setAttribute('aria-pressed', 'false');

    isConfigOpen = true;

    // Initialize config form if not already done
    if (currentSchema && currentConfig) {
      buildForm(currentSchema, currentConfig);
    } else {
      initConfigForm();
    }
  } else if (viewName === 'updates') {
    dashboardContent.style.opacity = '0';
    configContent.style.opacity = '0';
    setTimeout(() => {
      dashboardContent.style.display = 'none';
      configContent.style.display = 'none';
      logsContent.style.display = 'none';
      updatesContent.style.display = 'block';
      updatesContent.style.opacity = '1';
    }, 150);

    updatesBtn.classList.add('active');
    configBtn.classList.remove('active');
    dashboardBtn.classList.remove('active');
    logsBtn.classList.remove('active');
    updatesBtn.setAttribute('aria-pressed', 'true');
    configBtn.setAttribute('aria-pressed', 'false');
    dashboardBtn.setAttribute('aria-pressed', 'false');
    logsBtn.setAttribute('aria-pressed', 'false');

    // Auto refresh status and check for updates when opening
    (async () => {
      try {
        await fetch('/api/update/check', { method: 'POST' });
      } catch (e) {
        // eslint-disable-next-line no-console
        console.debug('update check failed', e);
      }
      try {
        const res = await fetch('/api/update/status');
        const s = await res.json();
        const statEl = $('updates_status');
        if (statEl && s) {
          if (!s.available) {
            statEl.textContent = 'Updater unavailable (not a Git deployment).';
          } else {
            const parts = [];
            if (s.branch) {
              parts.push(`branch: ${s.branch}`);
            }
            if (s.upstream) {
              parts.push(`upstream: ${s.upstream}`);
            }
            if (typeof s.behind === 'number') {
              parts.push(`behind: ${s.behind}`);
            }
            if (typeof s.ahead === 'number') {
              parts.push(`ahead: ${s.ahead}`);
            }
            if (s.head) {
              parts.push(`HEAD: ${s.head}`);
            }
            if (s.remote) {
              parts.push(`remote: ${s.remote}`);
            }
            statEl.textContent = parts.join(' | ');
          }
        }
      } catch (e) {
        // eslint-disable-next-line no-console
        console.debug('update status fetch failed', e);
      }
      try {
        if (typeof window.loadBranches === 'function') {
          window.loadBranches();
        }
      } catch (e) {
        // eslint-disable-next-line no-console
        console.debug('loadBranches error', e);
      }
    })();
  } else if (viewName === 'logs') {
    dashboardContent.style.opacity = '0';
    configContent.style.opacity = '0';
    updatesContent.style.opacity = '0';
    setTimeout(() => {
      dashboardContent.style.display = 'none';
      configContent.style.display = 'none';
      updatesContent.style.display = 'none';
      logsContent.style.display = 'block';
      logsContent.style.opacity = '1';
    }, 150);

    logsBtn.classList.add('active');
    configBtn.classList.remove('active');
    dashboardBtn.classList.remove('active');
    updatesBtn.classList.remove('active');
    logsBtn.setAttribute('aria-pressed', 'true');
    configBtn.setAttribute('aria-pressed', 'false');
    dashboardBtn.setAttribute('aria-pressed', 'false');
    updatesBtn.setAttribute('aria-pressed', 'false');

    ensureLogsStream();
  }
}

function initUX() {
  // View toggle functionality
  const dashboardBtn = $('dashboard_view');
  const configBtn = $('config_view');
  const updatesBtn = $('updates_view');
  const logsBtn = $('logs_view');
  const menuToggle = $('menu_toggle');
  const menuDropdown = $('menu_dropdown');

  if (dashboardBtn) {
    dashboardBtn.addEventListener('click', () => {
      switchView('dashboard');
      if (menuDropdown) {
        menuDropdown.style.display = 'none';
        if (menuToggle) {
          menuToggle.setAttribute('aria-expanded', 'false');
        }
      }
    });
    addButtonFeedback(dashboardBtn);
  }

  if (configBtn) {
    configBtn.addEventListener('click', () => {
      switchView('config');
      if (menuDropdown) {
        menuDropdown.style.display = 'none';
        if (menuToggle) {
          menuToggle.setAttribute('aria-expanded', 'false');
        }
      }
    });
    addButtonFeedback(configBtn);
  }

  if (updatesBtn) {
    updatesBtn.addEventListener('click', () => {
      switchView('updates');
      if (menuDropdown) {
        menuDropdown.style.display = 'none';
        if (menuToggle) {
          menuToggle.setAttribute('aria-expanded', 'false');
        }
      }
    });
    addButtonFeedback(updatesBtn);
  }

  if (logsBtn) {
    logsBtn.addEventListener('click', () => {
      switchView('logs');
      if (menuDropdown) {
        menuDropdown.style.display = 'none';
        if (menuToggle) {
          menuToggle.setAttribute('aria-expanded', 'false');
        }
      }
    });
    addButtonFeedback(logsBtn);
  }

  // Menu (hamburger) behavior
  if (menuToggle && menuDropdown) {
    menuToggle.addEventListener('click', () => {
      const isOpen = menuDropdown.style.display !== 'none';
      menuDropdown.style.display = isOpen ? 'none' : '';
      menuToggle.setAttribute('aria-expanded', String(!isOpen));
    });

    // Close on outside click
    document.addEventListener('click', e => {
      const target = e.target;
      if (!menuDropdown || !menuToggle) {
        return;
      }
      if (
        target !== menuDropdown &&
        target !== menuToggle &&
        !menuDropdown.contains(target) &&
        !menuToggle.contains(target)
      ) {
        if (menuDropdown.style.display !== 'none') {
          menuDropdown.style.display = 'none';
          menuToggle.setAttribute('aria-expanded', 'false');
        }
      }
    });
  }

  // Configuration functionality (advanced sections are always visible now)

  const saveBtn = $('save_config');
  if (saveBtn) {
    saveBtn.addEventListener('click', saveConfig);
    addButtonFeedback(saveBtn);
  }

  // Test vehicle configuration
  const testBtn = $('test_vehicle');
  if (testBtn) {
    testBtn.addEventListener('click', async () => {
      const statusEl = $('config_status');
      try {
        const payload = collectConfig(currentSchema);
        const res = await postJSON('/api/config/test-vehicle', payload, 'POST');
        const ok = !!res.ok;
        if (statusEl) {
          if (ok) {
            const v = res.vehicle || {};
            const name = v.name || '-';
            const vin = v.vin || '';
            const soc = v.soc !== undefined && v.soc !== null ? `${v.soc}%` : 'n/a';
            const reason = res.message || res.reason || '';
            statusEl.textContent = `✅ Vehicle test OK: ${name} ${
              vin ? '(' + vin + ')' : ''
            } SOC=${soc}${reason ? ' — ' + reason : ''}`;
            statusEl.style.background = 'rgba(16, 185, 129, 0.1)';
            statusEl.style.color = '#10b981';
          } else {
            const msg = res.error || res.message || res.reason || 'Test failed';
            statusEl.textContent = `❌ ${msg}`;
            statusEl.style.background = 'rgba(239, 68, 68, 0.1)';
            statusEl.style.color = '#ef4444';
          }
        }
      } catch (e) {
        if (statusEl) {
          statusEl.textContent = `❌ ${e && e.message ? e.message : 'Test failed'}`;
          statusEl.style.background = 'rgba(239, 68, 68, 0.1)';
          statusEl.style.color = '#ef4444';
        }
      }
    });
    addButtonFeedback(testBtn);
  }

  // Updates actions
  const btnCheck = $('btn_check_updates');
  const btnApply = $('btn_apply_updates');
  const statEl = $('updates_status');
  const branchSelect = $('branch_select');
  const btnSwitch = $('btn_switch_branch');
  async function refreshUpdateStatus() {
    try {
      const res = await fetch('/api/update/status');
      const s = await res.json();
      if (statEl) {
        if (!s || s.available === false) {
          statEl.textContent = 'Updater unavailable (not a Git deployment).';
        } else {
          const parts = [];
          if (s.branch) {
            parts.push(`branch: ${s.branch}`);
          }
          if (s.upstream) {
            parts.push(`upstream: ${s.upstream}`);
          }
          if (typeof s.behind === 'number') {
            parts.push(`behind: ${s.behind}`);
          }
          if (typeof s.ahead === 'number') {
            parts.push(`ahead: ${s.ahead}`);
          }
          if (s.head) {
            parts.push(`HEAD: ${s.head}`);
          }
          if (s.remote) {
            parts.push(`remote: ${s.remote}`);
          }
          statEl.textContent = parts.join(' | ');
        }
      }
    } catch (e) {
      if (statEl) {
        statEl.textContent = 'Failed to load update status';
      }
    }
  }

  async function loadBranches() {
    if (!branchSelect) {
      return;
    }
    try {
      const r = await fetch('/api/update/branches');
      const s = await r.json();
      branchSelect.innerHTML = '';
      const current = s && s.current;
      const available = !!(s && s.available);

      let options = [];
      // Remote branches first, labeled as origin/name but value is plain name
      if (Array.isArray(s?.remote) && s.remote.length > 0) {
        options = s.remote.map(name => ({ value: String(name), label: `origin/${String(name)}` }));
      }
      // Fallback/include local branches if remote is empty or as additional options
      if (Array.isArray(s?.local) && s.local.length > 0) {
        s.local.forEach(name => {
          const exists = options.some(o => o.value === String(name));
          if (!exists) {
            options.push({ value: String(name), label: String(name) });
          }
        });
      }
      // Ensure current branch appears even if lists are empty
      if (current && !options.some(o => o.value === String(current))) {
        options.unshift({ value: String(current), label: String(current) });
      }

      if (!available || options.length === 0) {
        const opt = document.createElement('option');
        opt.value = '';
        opt.textContent = available ? 'No branches found' : 'Updater unavailable';
        branchSelect.appendChild(opt);
        branchSelect.disabled = true;
        if (btnSwitch) {
          btnSwitch.disabled = true;
        }
        return;
      }

      branchSelect.disabled = false;
      if (btnSwitch) {
        btnSwitch.disabled = false;
      }

      options.forEach(o => {
        const opt = document.createElement('option');
        opt.value = o.value;
        opt.textContent = o.value === current ? `${o.label} (current)` : o.label;
        if (o.value === current) {
          opt.selected = true;
        }
        branchSelect.appendChild(opt);
      });
      // If nothing selected (e.g., current missing), select first
      if (!branchSelect.value && options.length > 0) {
        branchSelect.value = options[0].value;
      }
    } catch (e) {
      branchSelect.innerHTML = '';
      const opt = document.createElement('option');
      opt.value = '';
      opt.textContent = 'Failed to load branches';
      branchSelect.appendChild(opt);
      branchSelect.disabled = true;
      if (btnSwitch) {
        btnSwitch.disabled = true;
      }
    }
  }

  // Expose for updates view to trigger refresh on open
  window.loadBranches = loadBranches;

  if (btnCheck) {
    btnCheck.addEventListener('click', async () => {
      if (statEl) {
        statEl.textContent = 'Checking for updates...';
      }
      try {
        const res = await fetch('/api/update/check', { method: 'POST' });
        const s = await res.json();
        if (s && s.ok) {
          // API may return {ok, ...status fields} or {ok, status: {...}}
          const payload = s.status || s;
          const parts = [];
          if (payload.branch) {
            parts.push(`branch: ${payload.branch}`);
          }
          if (payload.upstream) {
            parts.push(`upstream: ${payload.upstream}`);
          }
          if (typeof payload.behind === 'number') {
            parts.push(`behind: ${payload.behind}`);
          }
          if (typeof payload.ahead === 'number') {
            parts.push(`ahead: ${payload.ahead}`);
          }
          if (payload.head) {
            parts.push(`HEAD: ${payload.head}`);
          }
          if (payload.remote) {
            parts.push(`remote: ${payload.remote}`);
          }
          if (statEl) {
            statEl.textContent = parts.join(' | ');
          }
        } else {
          if (statEl) {
            statEl.textContent = `Check failed: ${s && s.error ? s.error : 'unknown error'}`;
          }
        }
      } catch (e) {
        if (statEl) {
          statEl.textContent = 'Check failed';
        }
      }
    });
  }
  if (btnApply) {
    btnApply.addEventListener('click', async () => {
      if (statEl) {
        statEl.textContent = 'Applying updates and restarting...';
      }
      try {
        const res = await fetch('/api/update/apply', { method: 'POST' });
        const s = await res.json();
        if (s && s.ok) {
          if (statEl) {
            statEl.textContent = 'Restarting... please wait and reload in a few seconds.';
          }
          // Optionally, try to ping status and reload when it responds
          setTimeout(() => {
            location.reload();
          }, 7000);
        } else {
          if (statEl) {
            statEl.textContent = `Update failed: ${s && s.error ? s.error : 'unknown error'}`;
          }
        }
      } catch (e) {
        if (statEl) {
          statEl.textContent = 'Update failed';
        }
      }
    });
  }
  if (btnSwitch && branchSelect) {
    btnSwitch.addEventListener('click', async () => {
      const branch = branchSelect.value;
      if (!branch) {
        return;
      }
      if (statEl) {
        statEl.textContent = `Switching to ${branch}...`;
      }
      try {
        const res = await fetch('/api/update/checkout', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ branch }),
        });
        const s = await res.json();
        if (s && s.ok) {
          if (statEl) {
            statEl.textContent = `Switched to ${branch}.`;
          }
          await loadBranches();
          await refreshUpdateStatus();
        } else if (statEl) {
          statEl.textContent = `Switch failed: ${s && s.error ? s.error : 'unknown error'}`;
        }
      } catch (e) {
        if (statEl) {
          statEl.textContent = 'Switch failed';
        }
      }
    });
  }
  // Load initial status
  refreshUpdateStatus();
  if (branchSelect) {
    loadBranches();
  }

  // Quick settings cards should open corresponding section
  document.querySelectorAll('.quick-setting-card[data-section]').forEach(card => {
    card.addEventListener('click', () => {
      const key = card.getAttribute('data-section');
      switchView('config');
      // Wait a tick for form to build if needed
      setTimeout(() => openConfigSection(key), 50);
    });
  });

  // Start with dashboard view
  switchView('dashboard');
}

// Logs streaming via SSE
let logsEventSource = null;
let logsPaused = false;
function appendLogLine(line) {
  const el = $('logs_pre');
  if (!el) {
    return;
  }
  el.textContent += (el.textContent ? '\n' : '') + line;
  // Auto-scroll
  if (!logsPaused) {
    el.scrollTop = el.scrollHeight;
  }
}

function ensureLogsStream() {
  if (logsEventSource) {
    return;
  }
  const pauseChk = $('logs_pause');
  if (pauseChk) {
    pauseChk.addEventListener('change', () => {
      logsPaused = !!pauseChk.checked;
    });
  }
  const clearBtn = $('btn_clear_logs');
  if (clearBtn) {
    clearBtn.addEventListener('click', () => {
      const el = $('logs_pre');
      if (el) {
        el.textContent = '';
      }
    });
  }
  // Do not preload via REST to avoid duplication with SSE backlog
  // Start SSE
  try {
    logsEventSource = new EventSource('/api/logs/stream');
    logsEventSource.onmessage = ev => {
      if (typeof ev.data === 'string') {
        appendLogLine(ev.data);
      }
    };
    logsEventSource.onerror = () => {
      // Retry strategy: close and recreate after delay
      if (logsEventSource) {
        logsEventSource.close();
        logsEventSource = null;
      }
      setTimeout(ensureLogsStream, 2000);
    };
  } catch (e) {
    // eslint-disable-next-line no-console
    console.debug('EventSource init failed', e);
  }
}

// Removed old control functions that are no longer needed

// Kick off
resizeChartCanvas();
fetchStatus();
initConfigForm();
initUX();
// Reduce polling frequency to 2s to lower UI churn
setInterval(() => {
  resizeChartCanvas();
  fetchStatus();
}, 2000);

// Update session time more frequently when charging
setInterval(() => {
  const sessionTimeEl = $('session_time');
  if (sessionTimeEl && window.lastStatusData && window.lastStatusData.status === 2) {
    // Update time display if actively charging: prefer session.charging_time_sec
    const sess = window.lastStatusData.session || {};
    if (typeof sess.charging_time_sec === 'number' && sess.charging_time_sec >= 0) {
      const base = sess.charging_time_sec;
      const duration = Math.floor(
        base + (Date.now() / 1000 - (window.lastStatusData._last_update_ts || Date.now() / 1000))
      );
      const hours = Math.floor(duration / 3600);
      const minutes = Math.floor((duration % 3600) / 60);
      const seconds = duration % 60;
      sessionTimeEl.textContent = `${hours.toString().padStart(2, '0')}:${minutes
        .toString()
        .padStart(2, '0')}:${seconds.toString().padStart(2, '0')}`;
    } else if (sess && sess.start_ts) {
      const startTime = new Date(sess.start_ts).getTime();
      const duration = Math.floor((Date.now() - startTime) / 1000);
      const hours = Math.floor(duration / 3600);
      const minutes = Math.floor((duration % 3600) / 60);
      const seconds = duration % 60;
      sessionTimeEl.textContent = `${hours.toString().padStart(2, '0')}:${minutes
        .toString()
        .padStart(2, '0')}:${seconds.toString().padStart(2, '0')}`;
    }
  }
}, 1000);
