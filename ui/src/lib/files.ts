// Saving and opening files from the UI. In the desktop app files go through native dialogs;
// in a browser (development/tests) they use a download link and a file input.

import { isTauri, rpcNative } from './bridge';

export async function saveTextFile(filename: string, text: string, mime = 'application/json'): Promise<boolean> {
  if (isTauri) {
    const saved = await rpcNative<boolean>('save_text_file', { filename, text });
    return saved;
  }
  const url = URL.createObjectURL(new Blob([text], { type: mime }));
  const a = document.createElement('a');
  a.href = url;
  a.download = filename;
  document.body.appendChild(a);
  a.click();
  a.remove();
  setTimeout(() => URL.revokeObjectURL(url), 2000);
  return true;
}

/** Let the user pick a text file; resolves to its content, or null if cancelled. */
export function openTextFile(accept = '.json'): Promise<string | null> {
  return new Promise((resolve) => {
    const input = document.createElement('input');
    input.type = 'file';
    input.accept = accept;
    input.style.display = 'none';
    input.onchange = async () => {
      const f = input.files?.[0];
      input.remove();
      resolve(f ? await f.text() : null);
    };
    input.oncancel = () => {
      input.remove();
      resolve(null);
    };
    document.body.appendChild(input);
    input.click();
  });
}

/** Read an image as a data URL, refusing large files. */
export function openImageFile(maxBytes = 900_000): Promise<{ dataUrl: string } | { error: string } | null> {
  return new Promise((resolve) => {
    const input = document.createElement('input');
    input.type = 'file';
    input.accept = 'image/png,image/jpeg,image/gif,image/webp,image/svg+xml';
    input.style.display = 'none';
    input.onchange = () => {
      const f = input.files?.[0];
      input.remove();
      if (!f) return resolve(null);
      if (f.size > maxBytes) return resolve({ error: `That image is ${(f.size / 1024).toFixed(0)} KB. Please choose one under ${(maxBytes / 1024).toFixed(0)} KB (a small PNG or SVG works best).` });
      const r = new FileReader();
      r.onload = () => resolve({ dataUrl: String(r.result) });
      r.onerror = () => resolve({ error: 'The image could not be read.' });
      r.readAsDataURL(f);
    };
    input.oncancel = () => {
      input.remove();
      resolve(null);
    };
    document.body.appendChild(input);
    input.click();
  });
}
