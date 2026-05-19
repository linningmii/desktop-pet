const speech = document.querySelector('#speech');
const speechBubble = document.querySelector('#speech-bubble');
const speechBubblePath = document.querySelector('#speech-bubble-path');

let speechLines = {};
let currentSpeech = '';
let currentSpeechKey = '';

async function loadSpeechLines() {
  try {
    const response = await fetch('assets/pet/speech-lines.json');
    speechLines = await response.json();
  } catch {
    speechLines = {};
  }
}

function pickLine(lines) {
  if (!Array.isArray(lines) || lines.length === 0) {
    return '';
  }

  return lines[Math.floor(Math.random() * lines.length)];
}

function linesForState(state) {
  const activity = state.activity === 'slacking' ? 'slacking' : 'work';
  const language = state.language === 'en' ? 'en' : 'zh';
  const lines = speechLines[language] || speechLines.zh;

  if (state.behavior === 'stopped' && !state.stoppedShapeshift) {
    const mood = state.stoppedMood || 'calm';
    return lines?.stopped?.[activity]?.[mood] || lines?.stopped?.[activity]?.calm || [];
  }

  return [];
}


function clamp(value, min, max) {
  return Math.min(Math.max(value, min), max);
}

function buildSpeechBubblePath(metrics) {
  const { left, top, right, bottom, radius, tailX, tailHeight, tailWidth, isBelow } = metrics;
  const rootHalf = tailWidth / 2;

  if (isBelow) {
    return [
      `M ${left + radius} ${top}`,
      `H ${tailX - rootHalf}`,
      `C ${tailX - rootHalf * 0.5} ${top - 1} ${tailX - rootHalf * 0.28} ${top - tailHeight * 0.55} ${tailX} ${top - tailHeight}`,
      `C ${tailX + rootHalf * 0.32} ${top - tailHeight * 0.52} ${tailX + rootHalf * 0.55} ${top - 1} ${tailX + rootHalf} ${top}`,
      `H ${right - radius}`,
      `Q ${right} ${top} ${right} ${top + radius}`,
      `V ${bottom - radius}`,
      `Q ${right} ${bottom} ${right - radius} ${bottom}`,
      `H ${left + radius}`,
      `Q ${left} ${bottom} ${left} ${bottom - radius}`,
      `V ${top + radius}`,
      `Q ${left} ${top} ${left + radius} ${top}`,
      'Z',
    ].join(' ');
  }

  return [
    `M ${left + radius} ${top}`,
    `H ${right - radius}`,
    `Q ${right} ${top} ${right} ${top + radius}`,
    `V ${bottom - radius}`,
    `Q ${right} ${bottom} ${right - radius} ${bottom}`,
    `H ${tailX + rootHalf}`,
    `C ${tailX + rootHalf * 0.55} ${bottom + 1} ${tailX + rootHalf * 0.32} ${bottom + tailHeight * 0.52} ${tailX} ${bottom + tailHeight}`,
    `C ${tailX - rootHalf * 0.28} ${bottom + tailHeight * 0.55} ${tailX - rootHalf * 0.5} ${bottom + 1} ${tailX - rootHalf} ${bottom}`,
    `H ${left + radius}`,
    `Q ${left} ${bottom} ${left} ${bottom - radius}`,
    `V ${top + radius}`,
    `Q ${left} ${top} ${left + radius} ${top}`,
    'Z',
  ].join(' ');
}

function updateSpeechBubble(placement, tailPercent) {
  const width = document.documentElement.clientWidth || window.innerWidth;
  const height = document.documentElement.clientHeight || window.innerHeight;
  const scale = document.body.dataset.scale || 'small';
  const stroke = scale === 'large' ? 3.8 : 3.4;
  const tailHeight = scale === 'large' ? 17 : scale === 'medium' ? 15 : 13;
  const tailWidth = scale === 'large' ? 27 : scale === 'medium' ? 24 : 21;
  const inset = Math.ceil(stroke + 2);
  const isBelow = placement === 'below';
  const top = isBelow ? inset + tailHeight : inset;
  const bottom = isBelow ? height - inset : height - inset - tailHeight;
  const left = inset;
  const right = width - inset;
  const radius = Math.max(18, (bottom - top) / 2);
  const tailX = clamp(
    (tailPercent / 100) * width,
    left + radius + tailWidth / 2,
    right - radius - tailWidth / 2,
  );

  const bubblePath = buildSpeechBubblePath({
    left,
    top,
    right,
    bottom,
    radius,
    tailX,
    tailHeight,
    tailWidth,
    isBelow,
  });

  speechBubble.setAttribute('viewBox', `0 0 ${width} ${height}`);
  speechBubblePath.setAttribute('d', bubblePath);
  speechBubblePath.style.strokeWidth = stroke;

  document.body.style.setProperty('--speech-content-left', `${left + 8}px`);
  document.body.style.setProperty('--speech-content-right', `${left + 8}px`);
  document.body.style.setProperty('--speech-content-top', `${top + 5}px`);
  document.body.style.setProperty('--speech-content-bottom', `${height - bottom + 5}px`);
}

function speechKeyForState(state) {
  if (state.behavior === 'stopped' && !state.stoppedShapeshift) {
    return `stopped:${state.language}:${state.activity}:${state.stoppedMood}`;
  }

  return '';
}

function renderState(state) {
  document.body.classList.toggle('below', state.speechPlacement === 'below');
  document.body.classList.toggle('above', state.speechPlacement !== 'below');
  document.body.dataset.scale = state.speechScale || 'small';
  updateSpeechBubble(state.speechPlacement, state.speechTailPercent || 50);

  const key = state.talkWhenStopped ? speechKeyForState(state) : '';
  if (!key) {
    currentSpeech = '';
    currentSpeechKey = '';
    speech.textContent = '';
    document.body.classList.remove('talking');
    return;
  }

  if (key !== currentSpeechKey) {
    currentSpeech = pickLine(linesForState(state));
    currentSpeechKey = key;
  }

  speech.textContent = currentSpeech;
  document.body.classList.toggle('talking', Boolean(currentSpeech));
}

async function main() {
  await loadSpeechLines();

  const tauri = window.__TAURI__;
  if (tauri?.event?.listen) {
    await tauri.event.listen('pet-state', (event) => renderState(event.payload));
  }
}

main();
