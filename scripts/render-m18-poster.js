// Playwright CLI authoring helper. Use only in an isolated Vite browser.
async (page) => {
  if (await page.evaluate(() => "__TAURI_INTERNALS__" in window))
    throw new Error("Poster authoring requires an isolated browser preview");
  const download = page.waitForEvent("download");
  await page.evaluate(async () => {
    const { M18Scene } = await import(`/src/devices/m18/scene3d.ts?poster=${Date.now()}`);
    const {initialFixture:initial}=await import("/scripts/platform-fixtures.ts");
    await document.fonts.ready;
    const host = document.createElement("div");
    Object.assign(host.style, {
      position: "fixed", inset: "0", width: "1200px", height: "900px", zIndex: "100",
    });
    document.body.append(host);
    const scene = new M18Scene(host, () => {}, () => {});
    try {
      await scene.load();
      scene.setPage(initial, {});
      scene.setView("top");
      const anchor = document.createElement("a");
      anchor.href = scene.capturePng();
      anchor.download = "m18-poster.png";
      document.body.append(anchor);
      anchor.click();
      anchor.remove();
    } finally {
      scene.dispose();
      host.remove();
    }
  });
  await (await download).saveAs("src/assets/devices/m18/m18-poster.png");
}
