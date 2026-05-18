const pet = document.querySelector('#pet');
const image = document.querySelector('#pet-frame');

const animation = {
  idle: [],
  walk: [],
  mode: 'idle',
  idleMood: 'calm',
  frame: 0,
  lastFrameAt: 0
};

function normalizeAssetList(manifestPath, files) {
  if (!Array.isArray(files)) {
    return [];
  }

  const base = manifestPath.replace(/manifest\.json$/i, '');
  return files
    .filter((file) => typeof file === 'string' && file.trim())
    .map((file) => ({
      path: file,
      url: `file:///${base.replace(/\\/g, '/')}${encodeURIComponent(file).replace(/%2F/g, '/')}`
    }));
}

async function loadManifest() {
  try {
    const manifestPath = await window.desktopPet.getAssetManifestPath();
    const response = await fetch(`file:///${manifestPath.replace(/\\/g, '/')}`);
    const manifest = await response.json();
    animation.idle = normalizeAssetList(manifestPath, manifest.idle);
    animation.walk = normalizeAssetList(manifestPath, manifest.walk);
  } catch {
    animation.idle = [];
    animation.walk = [];
  }
}

function setMode(mode) {
  if (animation.mode === mode) {
    return;
  }
  animation.mode = mode;
  animation.frame = 0;
  animation.lastFrameAt = performance.now();
}

function getIdleFrame() {
  const moodPatterns = {
    happy: '乐',
    calm: '呆',
    angry: '怒',
    sorrow: '苦'
  };
  const pattern = moodPatterns[animation.idleMood];
  return animation.idle.find((frame) => frame.path.includes(pattern)) || animation.idle[0];
}

function renderFrame(now) {
  const frames = animation.mode === 'walk' ? animation.walk : animation.idle;
  if (frames.length === 0) {
    pet.classList.add('placeholder');
    image.removeAttribute('src');
    requestAnimationFrame(renderFrame);
    return;
  }

  pet.classList.remove('placeholder');
  if (animation.mode === 'idle') {
    const frame = getIdleFrame();
    if (frame && image.src !== frame.url) {
      image.src = frame.url;
    }
    requestAnimationFrame(renderFrame);
    return;
  }

  if (image.src !== frames[animation.frame].url) {
    image.src = frames[animation.frame].url;
  }

  if (now - animation.lastFrameAt > (animation.mode === 'walk' ? 160 : 650)) {
    animation.frame = (animation.frame + 1) % frames.length;
    animation.lastFrameAt = now;
    image.src = frames[animation.frame].url;
  }

  requestAnimationFrame(renderFrame);
}

window.desktopPet.onState((state) => {
  setMode(state.moving ? 'walk' : 'idle');
  animation.idleMood = state.idleMood || 'calm';
  pet.dataset.facing = state.facing;
  pet.dataset.behavior = state.behavior;
  pet.style.setProperty('--pet-zoom', `${state.size / state.baseSize}`);
  pet.style.setProperty('--bob-speed', `${Math.max(0.45, 1.2 - state.speed / 12)}s`);
});

loadManifest().then(() => requestAnimationFrame(renderFrame));
