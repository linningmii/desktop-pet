const { contextBridge, ipcRenderer } = require('electron');

contextBridge.exposeInMainWorld('desktopPet', {
  onState(callback) {
    ipcRenderer.on('pet-state', (_event, state) => callback(state));
  },
  getAssetManifestPath() {
    return ipcRenderer.invoke('get-asset-manifest');
  }
});
