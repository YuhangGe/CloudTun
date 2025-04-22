import { readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import type { PluginOption } from 'vite';
import { getTailwindThemeCss } from 'jinge-antd/colors';

export function Base64LoaderPlugin(): PluginOption {
  return {
    name: 'base64-loader',
    enforce: 'pre',
    async load(id) {
      const [path, query] = id.split('?');
      if (query != 'base64') return undefined;

      const data = await readFile(path);
      const base64 = data.toString('base64');

      return `export default '${base64}';`;
    },
  };
}

export function TailwindThemePlugin(): PluginOption {
  const themeCss = getTailwindThemeCss();
  return {
    name: 'tailwind-theme-generator',
    async config(config) {
      const css = await readFile(path.resolve(__dirname, '../src/tailwind.css'), 'utf-8');

      await writeFile(
        path.resolve(__dirname, '../src/style.css'),
        css.replace('/* GENERATED_THEME */', themeCss),
      );
      return config;
    },
  };
}
