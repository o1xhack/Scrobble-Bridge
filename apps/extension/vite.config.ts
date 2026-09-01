import { defineConfig } from "vite";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL(".", import.meta.url));

export default defineConfig({
  build: {
    emptyOutDir: true,
    rollupOptions: {
      input: {
        popup: resolve(root, "popup.html"),
        background: resolve(root, "src/background.ts"),
      },
      output: {
        entryFileNames: (chunk) =>
          chunk.name === "background"
            ? "background.js"
            : "assets/[name]-[hash].js",
        chunkFileNames: "assets/[name]-[hash].js",
        assetFileNames: "assets/[name]-[hash][extname]",
      },
    },
  },
  plugins: [
    {
      name: "copy-extension-files",
      async writeBundle() {
        const { copyFile, cp, mkdir } = await import("node:fs/promises");
        const files = ["manifest.json"];
        for (const file of files) {
          await copyFile(resolve(root, file), resolve(root, "dist", file));
        }
        const iconDirectory = resolve(root, "dist/icons");
        await mkdir(iconDirectory, { recursive: true });
        for (const size of [32, 64]) {
          await copyFile(
            resolve(root, `../desktop/src-tauri/icons/${size}x${size}.png`),
            resolve(iconDirectory, `${size}.png`),
          );
        }
        await copyFile(
          resolve(root, "icons/128.png"),
          resolve(iconDirectory, "128.png"),
        );
        await cp(resolve(root, "_locales"), resolve(root, "dist/_locales"), {
          recursive: true,
        });
      },
    },
  ],
});
