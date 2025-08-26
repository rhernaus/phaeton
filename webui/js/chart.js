// Chart history and drawing
window.chartHistory = {
  points: [],
  windowSec: 300,
  maxBufferSec: 21600,
  hoverT: null,
};

window.addHistoryPoint = function (s) {
  const t = Date.now() / 1000;
  const current = Number(s.ac_current || 0);
  let allowed = Number(s.set_current || 0);
  const station = Number(s.station_max_current || 0);
  const mode = Number(s.mode || 0);
  if (mode === 1 || mode === 2) {
    allowed = Number(s.applied_current ?? allowed);
  }
  chartHistory.points.push({ t, current, allowed, station });
  const cutoff = t - chartHistory.maxBufferSec;
  chartHistory.points = chartHistory.points.filter(p => p.t >= cutoff);
  window.drawChart();
};

window.drawDotOnChart = function (ctx, x, y, color) {
  ctx.fillStyle = color;
  ctx.beginPath();
  ctx.arc(x, y, 3, 0, Math.PI * 2);
  ctx.fill();
};

window.drawChart = function () {
  const canvas = $('chart');
  if (!canvas) return;
  const ctx = canvas.getContext('2d');
  const dpr = window.devicePixelRatio || 1;
  const W = canvas.width / dpr;
  const H = canvas.height / dpr;
  ctx.clearRect(0, 0, W, H);
  ctx.fillStyle = '#1a2332';
  ctx.fillRect(0, 0, W, H);
  if (chartHistory.points.length < 2) return;
  const tEnd = chartHistory.points[chartHistory.points.length - 1].t;
  const tMinDesired = tEnd - chartHistory.windowSec;
  const visible = chartHistory.points.filter(p => p.t >= tMinDesired);
  if (visible.length < 2) return;
  const tMin = visible[0].t;
  const tMax = visible[visible.length - 1].t;
  const tSpan = Math.max(1, tMax - tMin);
  let vMax = 0;
  visible.forEach(p => { vMax = Math.max(vMax, p.current, p.allowed, p.station); });
  vMax = Math.max(10, Math.ceil(vMax / 5) * 5);
  function mapX(t) { return 40 + ((t - tMin) / tSpan) * (W - 60); }
  function mapY(v) { return H - 20 - (v / vMax) * (H - 40); }
  ctx.strokeStyle = 'rgba(255,255,255,0.1)';
  ctx.lineWidth = 1;
  for (let i = 0; i <= 5; i++) {
    const y = mapY((vMax / 5) * i);
    ctx.beginPath(); ctx.moveTo(40, y); ctx.lineTo(W - 20, y); ctx.stroke();
  }
  function plot(color, key) {
    ctx.strokeStyle = color; ctx.lineWidth = 2; ctx.beginPath();
    visible.forEach((p, idx) => { const x = mapX(p.t); const y = mapY(p[key]); if (idx === 0) ctx.moveTo(x, y); else ctx.lineTo(x, y); });
    ctx.stroke();
  }
  plot('#22c55e', 'current');
  plot('#f59e0b', 'allowed');
  plot('#ef4444', 'station');
  ctx.strokeStyle = 'rgba(255,255,255,0.2)'; ctx.lineWidth = 1; ctx.beginPath(); ctx.moveTo(40, 10); ctx.lineTo(40, H - 20); ctx.lineTo(W - 20, H - 20); ctx.stroke();
  ctx.fillStyle = '#8899aa'; ctx.font = '12px -apple-system, sans-serif'; ctx.fillText(`${vMax} A`, 4, mapY(vMax) + 4); ctx.fillText('0', 20, H - 22);
  const numTicks = 5; const step = tSpan / numTicks; ctx.fillStyle = '#94a3b8';
  for (let i = 0; i <= numTicks; i++) { const tTick = tMin + step * i; const x = mapX(tTick); ctx.strokeStyle = 'rgba(255,255,255,0.15)'; ctx.beginPath(); ctx.moveTo(x, H - 20); ctx.lineTo(x, H - 16); ctx.stroke(); const d = new Date(tTick * 1000); const hh = String(d.getHours()).padStart(2, '0'); const mm = String(d.getMinutes()).padStart(2, '0'); const label = `${hh}:${mm}`; const textW = ctx.measureText(label).width; ctx.fillText(label, Math.min(Math.max(40, x - textW / 2), W - 20 - textW), H - 4); }
  const tip = $('chart_tooltip');
  if (chartHistory.hoverT && tip) {
    let nearest = visible[0]; let bestDt = Math.abs(chartHistory.hoverT - nearest.t);
    for (let i = 1; i < visible.length; i++) { const dt = Math.abs(chartHistory.hoverT - visible[i].t); if (dt < bestDt) { bestDt = dt; nearest = visible[i]; } }
    const x = mapX(nearest.t);
    ctx.strokeStyle = 'rgba(148,163,184,0.6)'; ctx.beginPath(); ctx.moveTo(x, 10); ctx.lineTo(x, H - 20); ctx.stroke();
    drawDotOnChart(ctx, x, mapY(nearest.current), '#22c55e');
    drawDotOnChart(ctx, x, mapY(nearest.allowed), '#f59e0b');
    drawDotOnChart(ctx, x, mapY(nearest.station), '#ef4444');
    const d = new Date(nearest.t * 1000); const hh = String(d.getHours()).padStart(2, '0'); const mm = String(d.getMinutes()).padStart(2, '0'); const ss = String(d.getSeconds()).padStart(2, '0');
    tip.innerHTML = `${hh}:${mm}:${ss} — cur ${nearest.current.toFixed(1)} A · allow ${nearest.allowed.toFixed(1)} A · max ${nearest.station.toFixed(0)} A`;
    const rect = canvas.getBoundingClientRect(); const parent = canvas.parentElement; const parentRect = parent ? parent.getBoundingClientRect() : { left: 0, top: 0, width: rect.width };
    const canvasCssW = rect.width; const scale = canvasCssW / W; const cssX = x * scale + (rect.left - parentRect.left); const top = rect.top - parentRect.top + 12;
    tip.style.left = `${cssX}px`; tip.style.top = `${top}px`; tip.style.display = '';
  } else if (tip) { tip.style.display = 'none'; }
};

window.resizeChartCanvas = function () {
  const canvas = $('chart'); if (!canvas) return; const dpr = window.devicePixelRatio || 1; if (window.chartDevicePixelRatio === dpr && canvas.dataset.sized === '1') return; const rect = canvas.getBoundingClientRect(); const cssWidth = Math.floor(rect.width); const cssHeight = Math.floor(rect.height); canvas.width = Math.max(320, cssWidth) * dpr; canvas.height = Math.max(120, cssHeight) * dpr; const ctx = canvas.getContext('2d'); ctx.setTransform(dpr, 0, 0, dpr, 0, 0); canvas.dataset.sized = '1'; window.chartDevicePixelRatio = dpr; window.drawChart();
};

window.chartDevicePixelRatio = 0;
window.addEventListener('resize', () => { window.chartDevicePixelRatio = 0; window.resizeChartCanvas(); });

// Chart controls and hover
document.getElementById('range')?.addEventListener('change', e => {
  window.chartHistory.windowSec = parseInt(e.target.value, 10) || 300;
  window.drawChart();
});

(function initChartHover() {
  const canvas = document.getElementById('chart');
  if (!canvas) return;
  canvas.addEventListener('mousemove', e => {
    const rect = canvas.getBoundingClientRect();
    const cssX = e.clientX - rect.left;
    if (chartHistory.points.length < 2) return;
    const tEnd = chartHistory.points[chartHistory.points.length - 1].t;
    const tMinDesired = tEnd - chartHistory.windowSec;
    const visible = chartHistory.points.filter(p => p.t >= tMinDesired);
    if (visible.length < 2) return;
    const tMin = visible[0].t;
    const tMax = visible[visible.length - 1].t;
    const tSpan = Math.max(1, tMax - tMin);
    const rectW = rect.width;
    const x = Math.max(40, Math.min(rectW - 20, cssX));
    const frac = (x - 40) / Math.max(1, rectW - 60);
    chartHistory.hoverT = tMin + frac * tSpan;
    window.drawChart();
  });
  canvas.addEventListener('mouseleave', () => {
    chartHistory.hoverT = null;
    window.drawChart();
  });
})();


