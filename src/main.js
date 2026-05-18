const path = require('node:path');
const { app, BrowserWindow, Menu, Tray, nativeImage, screen, ipcMain } = require('electron');

const BASE_PET_SIZE = 150;
const PET_SIZES = {
  small: 48,
  medium: 96,
  large: 192
};
const SPEED_PROFILES = {
  slow: 0.6,
  normal: 1.2,
  fast: 2.4
};
const AVOID_RADIUS_BY_SIZE = {
  small: 96,
  medium: 170,
  large: 280
};
const TICK_MS = 33;
const PATROL_SPEED = 2.8;
const PATROL_ACCELERATION = 0.18;
const ESCAPE_FORCE = 5.2;
const MAX_PATROL_SPEED = 4.2;
const MAX_ESCAPE_SPEED = 13.5;
const FRICTION = 0.94;
const EDGE_PADDING = 8;
const START_DURATION_MS = 900;
const STOP_DURATION_MS = 850;
const IDLE_MIN_MS = 5000;
const IDLE_MAX_MS = 20000;
const PATROL_MIN_MS = 3000;
const PATROL_MAX_MS = 60000;
const IDLE_SHAPESHIFT_CHANCE = 0.3;
const IDLE_SHAPESHIFT_INTERVAL_MS = 1000;
const ACTIVITY_PROFILES = {
  work: {
    label: '工作',
    weights: {
      happy: 1,
      calm: 9,
      angry: 60,
      sorrow: 30
    }
  },
  slacking: {
    label: '摸鱼',
    weights: {
      happy: 40,
      calm: 40,
      angry: 5,
      sorrow: 15
    }
  }
};

let petWindow;
let tray;
let isQuitting = false;
let petConfig = {
  size: 'small',
  speed: 'fast',
  activity: 'work'
};
let petState = {
  x: 0,
  y: 0,
  vx: 2,
  vy: 0,
  facing: 'right',
  moving: true
};
let patrolDirection = -1;
let patrolY = 0;
let wasEscaping = false;
let behavior = 'patrol';
let behaviorStartedAt = 0;
let nextBehaviorChangeAt = 0;
let idleMood = 'calm';
let idleShapeshift = false;
let nextIdleMoodChangeAt = 0;
let topmostInterval;
let lastTopmostAt = 0;

function randomBetween(min, max) {
  return min + Math.random() * (max - min);
}

function easeInOut(t) {
  const progress = Math.max(0, Math.min(1, t));
  return progress * progress * (3 - 2 * progress);
}

function pickWeighted(weights) {
  const entries = Object.entries(weights);
  const total = entries.reduce((sum, [, weight]) => sum + weight, 0);
  let target = Math.random() * total;
  for (const [key, weight] of entries) {
    target -= weight;
    if (target <= 0) {
      return key;
    }
  }
  return entries[entries.length - 1][0];
}

function chooseIdleMood() {
  idleMood = pickWeighted(ACTIVITY_PROFILES[petConfig.activity].weights);
}

function setBehavior(nextBehavior) {
  behavior = nextBehavior;
  behaviorStartedAt = Date.now();

  if (nextBehavior === 'idle') {
    idleShapeshift = Math.random() < IDLE_SHAPESHIFT_CHANCE;
    chooseIdleMood();
    nextIdleMoodChangeAt = behaviorStartedAt + IDLE_SHAPESHIFT_INTERVAL_MS;
    nextBehaviorChangeAt = behaviorStartedAt + randomBetween(IDLE_MIN_MS, IDLE_MAX_MS);
  } else {
    idleShapeshift = false;
    if (nextBehavior === 'patrol') {
      nextBehaviorChangeAt = behaviorStartedAt + randomBetween(PATROL_MIN_MS, PATROL_MAX_MS);
    }
  }
}

function getPetSize() {
  return PET_SIZES[petConfig.size];
}

function getSpeedMultiplier() {
  return SPEED_PROFILES[petConfig.speed];
}

function getAvoidRadius() {
  return AVOID_RADIUS_BY_SIZE[petConfig.size];
}

function createTrayIcon() {
  const trayIconPath = path.join(app.getAppPath(), 'assets', 'tray-icon.png');
  const fileIcon = nativeImage.createFromPath(trayIconPath);
  if (!fileIcon.isEmpty()) {
    return fileIcon.resize({ width: 16, height: 16 });
  }

  const svg = `
    <svg xmlns="http://www.w3.org/2000/svg" width="32" height="32" viewBox="0 0 32 32">
      <path d="M9 10 7 3l7 4M23 10l2-7-7 4" fill="#f59f2a" stroke="#5f330f" stroke-width="2" stroke-linejoin="round"/>
      <ellipse cx="16" cy="18" rx="11" ry="10" fill="#ffbd38" stroke="#5f330f" stroke-width="2"/>
      <path d="M19 10c3 1 5 3 6 6" fill="none" stroke="#ffe8a3" stroke-width="2" stroke-linecap="round"/>
      <circle cx="12" cy="16" r="1.8" fill="#2a1a10"/>
      <circle cx="20" cy="16" r="1.8" fill="#2a1a10"/>
      <path d="M12 22c2 2 6 2 8 0" fill="none" stroke="#2a1a10" stroke-width="2" stroke-linecap="round"/>
    </svg>`;
  return nativeImage.createFromDataURL(`data:image/svg+xml;charset=utf-8,${encodeURIComponent(svg)}`);
}

function applyPetSize(size) {
  const previousSize = getPetSize();
  petConfig.size = size;
  const nextSize = getPetSize();
  const footOffset = previousSize - nextSize;
  petState.x += footOffset / 2;
  petState.y += footOffset;
  patrolY += footOffset;

  if (petWindow && !petWindow.isDestroyed()) {
    const bounds = getBoundsForPet();
    clampPatrolY(bounds);
    clampToBounds(bounds);
    petWindow.setBounds({
      x: Math.round(petState.x),
      y: Math.round(petState.y),
      width: nextSize,
      height: nextSize
    }, false);
    petWindow.webContents.send('pet-state', buildRendererState(false));
  }

  updateTrayMenu();
}

function applyPetSpeed(speed) {
  petConfig.speed = speed;
  updateTrayMenu();
}

function applyActivity(activity) {
  petConfig.activity = activity;
  if (behavior === 'idle') {
    chooseIdleMood();
    if (petWindow && !petWindow.isDestroyed()) {
      petWindow.webContents.send('pet-state', buildRendererState(false));
    }
  }
  updateTrayMenu();
}

function buildTrayMenu() {
  return Menu.buildFromTemplate([
    {
      label: '显示宠物',
      click: () => {
        if (petWindow) {
          petWindow.showInactive();
        }
      }
    },
    { type: 'separator' },
    {
      label: '大小',
      submenu: [
        { label: '小', type: 'radio', checked: petConfig.size === 'small', click: () => applyPetSize('small') },
        { label: '中', type: 'radio', checked: petConfig.size === 'medium', click: () => applyPetSize('medium') },
        { label: '大', type: 'radio', checked: petConfig.size === 'large', click: () => applyPetSize('large') }
      ]
    },
    {
      label: '运动速度',
      submenu: [
        { label: '慢', type: 'radio', checked: petConfig.speed === 'slow', click: () => applyPetSpeed('slow') },
        { label: '正常', type: 'radio', checked: petConfig.speed === 'normal', click: () => applyPetSpeed('normal') },
        { label: '快', type: 'radio', checked: petConfig.speed === 'fast', click: () => applyPetSpeed('fast') }
      ]
    },
    {
      label: `我在${ACTIVITY_PROFILES[petConfig.activity].label}`,
      submenu: [
        { label: '工作', type: 'radio', checked: petConfig.activity === 'work', click: () => applyActivity('work') },
        { label: '摸鱼', type: 'radio', checked: petConfig.activity === 'slacking', click: () => applyActivity('slacking') }
      ]
    },
    { type: 'separator' },
    {
      label: '退出',
      click: () => {
        isQuitting = true;
        app.quit();
      }
    }
  ]);
}

function updateTrayMenu() {
  tray.setContextMenu(buildTrayMenu());
}

function createTray() {
  tray = new Tray(createTrayIcon());
  tray.setToolTip('Desktop Pet');
  tray.setContextMenu(buildTrayMenu());
}

function ensurePetTopmost(force = false) {
  if (!petWindow || petWindow.isDestroyed()) {
    return;
  }

  const now = Date.now();
  if (!force && now - lastTopmostAt < 500) {
    return;
  }

  lastTopmostAt = now;
  petWindow.setAlwaysOnTop(true, 'screen-saver');
  petWindow.moveTop();
}

function createPetWindow() {
  const primaryDisplay = screen.getPrimaryDisplay();
  const { x, y, width, height } = primaryDisplay.workArea;
  const petSize = getPetSize();
  const appIconPath = path.join(app.getAppPath(), 'assets', 'app-icon.ico');
  petState.x = x + width - petSize - 80;
  petState.y = y + height - petSize - 60;
  patrolY = petState.y;
  setBehavior('patrol');

  petWindow = new BrowserWindow({
    width: petSize,
    height: petSize,
    x: Math.round(petState.x),
    y: Math.round(petState.y),
    frame: false,
    transparent: true,
    resizable: false,
    movable: false,
    fullscreenable: false,
    skipTaskbar: true,
    hasShadow: false,
    alwaysOnTop: true,
    focusable: false,
    show: false,
    icon: appIconPath,
    webPreferences: {
      preload: path.join(__dirname, 'preload.js'),
      contextIsolation: true,
      nodeIntegration: false
    }
  });

  ensurePetTopmost(true);
  petWindow.setVisibleOnAllWorkspaces(true, { visibleOnFullScreen: false });
  petWindow.setIgnoreMouseEvents(true, { forward: true });
  petWindow.removeMenu();
  petWindow.loadFile(path.join(__dirname, 'renderer.html'));
  petWindow.once('ready-to-show', () => {
    petWindow.showInactive();
    ensurePetTopmost(true);
  });

  petWindow.on('close', (event) => {
    if (!isQuitting) {
      event.preventDefault();
      petWindow.hide();
    }
  });
}

function getVirtualWorkArea() {
  const displays = screen.getAllDisplays();
  const areas = displays.map((display) => display.workArea);
  const minX = Math.min(...areas.map((area) => area.x));
  const minY = Math.min(...areas.map((area) => area.y));
  const maxX = Math.max(...areas.map((area) => area.x + area.width));
  const maxY = Math.max(...areas.map((area) => area.y + area.height));

  return {
    x: minX,
    y: minY,
    width: maxX - minX,
    height: maxY - minY
  };
}

function getBoundsForPet() {
  return getVirtualWorkArea();
}

function clampToBounds(bounds) {
  const petSize = getPetSize();
  const minX = bounds.x + EDGE_PADDING;
  const minY = bounds.y + EDGE_PADDING;
  const maxX = bounds.x + bounds.width - petSize - EDGE_PADDING;
  const maxY = bounds.y + bounds.height - petSize - EDGE_PADDING;

  if (petState.x < minX) {
    petState.x = minX;
    petState.vx = Math.abs(petState.vx) + 1;
  }
  if (petState.x > maxX) {
    petState.x = maxX;
    petState.vx = -Math.abs(petState.vx) - 1;
  }
  if (petState.y < minY) {
    petState.y = minY;
    petState.vy = Math.abs(petState.vy) + 1;
  }
  if (petState.y > maxY) {
    petState.y = maxY;
    petState.vy = -Math.abs(petState.vy) - 1;
  }
}

function clampPatrolY(bounds) {
  const petSize = getPetSize();
  const minY = bounds.y + EDGE_PADDING;
  const maxY = bounds.y + bounds.height - petSize - EDGE_PADDING;
  patrolY = Math.max(minY, Math.min(maxY, patrolY));
}

function buildRendererState(isEscaping) {
  return {
    facing: petState.facing,
    moving: petState.moving,
    behavior: isEscaping ? 'escaping' : behavior,
    idleMood,
    idleShapeshift,
    activity: petConfig.activity,
    size: getPetSize(),
    baseSize: BASE_PET_SIZE,
    speed: Math.hypot(petState.vx, petState.vy)
  };
}

function updatePetMotion() {
  if (!petWindow || petWindow.isDestroyed()) {
    return;
  }

  const now = Date.now();
  const petSize = getPetSize();
  const speedMultiplier = getSpeedMultiplier();
  const avoidRadius = getAvoidRadius();
  const cursor = screen.getCursorScreenPoint();
  const centerX = petState.x + petSize / 2;
  const centerY = petState.y + petSize / 2;
  const dx = centerX - cursor.x;
  const dy = centerY - cursor.y;
  const distance = Math.hypot(dx, dy);
  const isEscaping = distance < avoidRadius;
  const justStoppedEscaping = wasEscaping && !isEscaping;

  if (isEscaping) {
    const strength = (avoidRadius - Math.max(distance, 1)) / avoidRadius;
    petState.vx += (dx / Math.max(distance, 1)) * strength * ESCAPE_FORCE * speedMultiplier;
    petState.vy += (dy / Math.max(distance, 1)) * strength * ESCAPE_FORCE * speedMultiplier;
  } else {
    if (justStoppedEscaping) {
      patrolY = petState.y;
      petState.vy = 0;
    }

    let behaviorElapsed = now - behaviorStartedAt;
    if (behavior === 'idle' && now > nextBehaviorChangeAt) {
      setBehavior('starting');
    } else if (behavior === 'idle' && idleShapeshift && now > nextIdleMoodChangeAt) {
      chooseIdleMood();
      nextIdleMoodChangeAt = now + IDLE_SHAPESHIFT_INTERVAL_MS;
    } else if (behavior === 'starting' && behaviorElapsed > START_DURATION_MS) {
      setBehavior('patrol');
    } else if (behavior === 'patrol' && now > nextBehaviorChangeAt) {
      setBehavior('stopping');
    } else if (behavior === 'stopping' && behaviorElapsed > STOP_DURATION_MS) {
      setBehavior('idle');
    }

    behaviorElapsed = now - behaviorStartedAt;
    let patrolForce = 0;
    if (behavior === 'starting') {
      patrolForce = PATROL_ACCELERATION * speedMultiplier * easeInOut(behaviorElapsed / START_DURATION_MS);
    } else if (behavior === 'patrol') {
      patrolForce = PATROL_ACCELERATION * speedMultiplier;
    } else if (behavior === 'stopping') {
      const stopProgress = easeInOut(behaviorElapsed / STOP_DURATION_MS);
      patrolForce = PATROL_ACCELERATION * speedMultiplier * (1 - stopProgress) * 0.45;
      petState.vx *= 1 - stopProgress * 0.09;
    } else {
      petState.vx *= 0.82;
    }

    petState.vx += patrolDirection * patrolForce;
    petState.vy += (patrolY - petState.y) * 0.08;
  }

  petState.vx *= FRICTION;
  petState.vy *= isEscaping ? FRICTION : 0.72;

  const speed = Math.hypot(petState.vx, petState.vy);
  const behaviorElapsed = now - behaviorStartedAt;
  let maxSpeed = MAX_PATROL_SPEED;
  if (isEscaping) {
    maxSpeed = MAX_ESCAPE_SPEED * speedMultiplier;
  } else if (behavior === 'starting') {
    maxSpeed = Math.max(
      PATROL_SPEED * speedMultiplier * 0.35,
      MAX_PATROL_SPEED * speedMultiplier * easeInOut(behaviorElapsed / START_DURATION_MS)
    );
  } else if (behavior === 'stopping' || behavior === 'idle') {
    maxSpeed = Math.max(0.18, MAX_PATROL_SPEED * speedMultiplier * (1 - easeInOut(behaviorElapsed / STOP_DURATION_MS)));
  } else {
    maxSpeed = MAX_PATROL_SPEED * speedMultiplier;
  }

  if (speed > maxSpeed) {
    petState.vx = (petState.vx / speed) * maxSpeed;
    petState.vy = (petState.vy / speed) * maxSpeed;
  }

  petState.x += petState.vx;
  petState.y += petState.vy;
  const bounds = getBoundsForPet();
  clampPatrolY(bounds);
  clampToBounds(bounds);

  if (!isEscaping) {
    const minX = bounds.x + EDGE_PADDING;
    const maxX = bounds.x + bounds.width - petSize - EDGE_PADDING;

    petState.y = patrolY;
    petState.vy = 0;

    if (petState.x <= minX + 2) {
      patrolDirection = 1;
      petState.vx = Math.max(PATROL_SPEED * speedMultiplier, Math.abs(petState.vx));
    } else if (petState.x >= maxX - 2) {
      patrolDirection = -1;
      petState.vx = -Math.max(PATROL_SPEED * speedMultiplier, Math.abs(petState.vx));
    }
  } else {
    patrolDirection = petState.vx >= 0 ? 1 : -1;
  }

  if (Math.abs(petState.vx) > 0.25) {
    petState.facing = petState.vx >= 0 ? 'right' : 'left';
  }
  petState.moving = isEscaping || Math.hypot(petState.vx, petState.vy) > 0.8;

  petWindow.setBounds({
    x: Math.round(petState.x),
    y: Math.round(petState.y),
    width: petSize,
    height: petSize
  }, false);
  ensurePetTopmost();
  petWindow.webContents.send('pet-state', buildRendererState(isEscaping));
  wasEscaping = isEscaping;
}

app.whenReady().then(() => {
  if (process.platform === 'darwin' && app.dock) {
    app.dock.hide();
  }

  Menu.setApplicationMenu(null);
  createTray();
  createPetWindow();
  setInterval(updatePetMotion, TICK_MS);
  topmostInterval = setInterval(() => ensurePetTopmost(true), 1000);

  app.on('activate', () => {
    if (BrowserWindow.getAllWindows().length === 0) {
      createPetWindow();
    }
  });
});

app.on('window-all-closed', (event) => {
  event.preventDefault();
});

ipcMain.handle('get-asset-manifest', async () => {
  return path.join(app.getAppPath(), 'assets', 'pet', 'manifest.json');
});
