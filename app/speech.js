const speech = document.querySelector('#speech');

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

function speechKeyForState(state) {
  if (state.behavior === 'stopped' && !state.stoppedShapeshift) {
    return `stopped:${state.language}:${state.activity}:${state.stoppedMood}`;
  }

  return '';
}

function renderState(state) {
  document.body.classList.toggle('below', state.speechPlacement === 'below');
  document.body.classList.toggle('above', state.speechPlacement !== 'below');

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
