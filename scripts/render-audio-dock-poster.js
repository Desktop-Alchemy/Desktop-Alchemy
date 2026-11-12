// Isolated Vite browser authoring helper; uses the production scene and transparent background.
async function renderAudioDockPoster(page) {
  if (await page.evaluate(() => !!window.__TAURI_INTERNALS__?.metadata))
    throw new Error("Use an isolated browser, not the native client");
  const download = page.waitForEvent("download");
  await page.evaluate(async () => {
    const { AudioDockScene } = await import(
      `/src/devices/ms-audio-dock/scene3d.ts?poster=${Date.now()}`
    );
    const host = document.createElement("div");
    Object.assign(host.style, {
      position: "fixed",
      inset: "0",
      width: "960px",
      height: "720px",
    });
    document.body.append(host);
    const scene = new AudioDockScene(host, () => {});
    try {
      await scene.load();
      scene.setView("top");
      const anchor = document.createElement("a");
      anchor.href = scene.capturePng(480, 360);
      anchor.download = "audio-dock-poster.png";
      anchor.click();
    } finally {
      scene.dispose();
      host.remove();
    }
  });
  await (
    await download
  ).saveAs("src/assets/devices/ms-audio-dock/audio-dock-poster.png");
}
