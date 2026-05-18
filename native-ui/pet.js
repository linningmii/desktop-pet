const pet = document.querySelector('#pet');
const image = document.querySelector('#pet-frame');

let manifest = { idle: {}, walk: [] };
let walkFrameIndex = 0;
let lastWalkFrameAt = 0;
let currentIdleSrc = '';
let currentIdleMood = '';

function encodeAssetPath(path) {
  return path
    .split('/')
    .map((segment) => encodeURIComponent(segment))
    .join('/');
}

function normalizeFrames(files) {
  if (!Array.isArray(files)) {
    return [];
  }

  return files
    .filter((file) => typeof file === 'string' && file.trim())
    .map((file) => `assets/pet/${encodeAssetPath(file)}`);
}

function buildIdleManifest(idle) {
  const moodPatterns = {
    happy: '乐',
    calm: '呆',
    angry: '怒',
    sorrow: '苦'
  };

  if (Array.isArray(idle)) {
    const frames = normalizeFrames(idle);
    return Object.fromEntries(
      Object.entries(moodPatterns).map(([mood, pattern]) => {
        const matched = frames.filter((frame) => decodeURIComponent(frame).includes(pattern));
        return [mood, matched.length > 0 ? matched : frames];
      })
    );
  }

  return Object.fromEntries(
    Object.entries(idle || {}).map(([mood, files]) => [mood, normalizeFrames(files)])
  );
}

async function loadManifest() {
  try {
    const response = await fetch('assets/pet/manifest.json');
    const data = await response.json();
    manifest = {
      idle: buildIdleManifest(data.idle),
      walk: normalizeFrames(data.walk || [])
    };
  } catch {
    manifest = { idle: {}, walk: [] };
  }
}

function setFrame(src) {
  if (!src) {
    image.removeAttribute('src');
    pet.classList.add('placeholder');
    return;
  }

  if (image.getAttribute('src') !== src) {
    image.src = src;
  }
  pet.classList.remove('placeholder');
}

function pickIdleFrame(mood) {
  const frames = manifest.idle[mood] || [];
  if (frames.length === 0) {
    return '';
  }

  return frames[Math.floor(Math.random() * frames.length)];
}

function updateFrame(state) {
  const now = performance.now();
  const isMoving = state.moving || state.behavior === 'escaping';

  if (isMoving && manifest.walk.length > 0) {
    if (now - lastWalkFrameAt > 160) {
      walkFrameIndex = (walkFrameIndex + 1) % manifest.walk.length;
      lastWalkFrameAt = now;
    }
    currentIdleSrc = '';
    currentIdleMood = '';
    setFrame(manifest.walk[walkFrameIndex]);
    return;
  }

  if (!currentIdleSrc || currentIdleMood !== state.idleMood || state.idleShapeshift) {
    currentIdleSrc = pickIdleFrame(state.idleMood);
    currentIdleMood = state.idleMood;
  }
  setFrame(currentIdleSrc);
}

function renderState(state) {
  pet.dataset.facing = state.facing || 'right';
  pet.classList.toggle('moving', Boolean(state.moving));
  pet.style.setProperty('--bob-speed', `${Math.max(0.45, 1.2 - (state.speed || 0) / 12)}s`);
  updateFrame(state);
}

async function main() {
  await loadManifest();

  const tauri = window.__TAURI__;
  if (tauri?.event?.listen) {
    await tauri.event.listen('pet-state', (event) => renderState(event.payload));
  }
}

main();
