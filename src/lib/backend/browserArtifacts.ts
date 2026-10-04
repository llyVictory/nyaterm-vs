/** Browser file workflows never pass client paths to the server. */
export function pickBrowserFile(accept: string): Promise<File | null> {
  const input = document.createElement("input");
  input.type = "file";
  input.accept = accept;
  input.hidden = true;
  document.body.append(input);
  return new Promise((resolve, reject) => {
    let focusTimer: ReturnType<typeof setTimeout> | undefined;
    const finish = (file: File | null) => {
      if (focusTimer) clearTimeout(focusTimer);
      window.removeEventListener("focus", onFocus);
      input.remove();
      resolve(file);
    };
    const onFocus = () => {
      focusTimer = setTimeout(() => finish(input.files?.[0] ?? null), 400);
    };
    if (!("oncancel" in input)) window.addEventListener("focus", onFocus, { once: true });
    input.onchange = () => finish(input.files?.[0] ?? null);
    input.oncancel = () => finish(null);
    try {
      input.click();
    } catch (error) {
      window.removeEventListener("focus", onFocus);
      input.remove();
      reject(error);
    }
  });
}

export function downloadJson(name: string, value: unknown): void {
  downloadBlob(name, new Blob([JSON.stringify(value, null, 2)], { type: "application/json" }));
}

export function downloadBlob(name: string, blob: Blob): void {
  const url = URL.createObjectURL(blob);
  const link = document.createElement("a");
  link.href = url;
  link.download = name;
  document.body.append(link);
  try {
    link.click();
  } finally {
    link.remove();
    window.setTimeout(() => URL.revokeObjectURL(url), 30_000);
  }
}

export async function readBrowserJson<T>(file: File): Promise<T> {
  if (file.size > 1024 * 1024) throw new Error("JSON file exceeds 1 MiB");
  return JSON.parse(await file.text()) as T;
}

export async function pickBrowserImage(
  maxDimension = 256,
): Promise<{ name: string; dataUrl: string } | null> {
  const file = await pickBrowserFile("image/png,image/jpeg,image/webp,image/gif,image/bmp");
  if (!file) return null;
  if (file.size > 10 * 1024 * 1024) throw new Error("Image exceeds 10 MiB");
  const bitmap = await createImageBitmap(file);
  try {
    if (!bitmap.width || !bitmap.height || bitmap.width * bitmap.height > 40_000_000)
      throw new Error("Image dimensions exceed limit");
    const scale = Math.min(1, maxDimension / Math.max(bitmap.width, bitmap.height));
    const canvas = document.createElement("canvas");
    canvas.width = Math.max(1, Math.round(bitmap.width * scale));
    canvas.height = Math.max(1, Math.round(bitmap.height * scale));
    const context = canvas.getContext("2d");
    if (!context) throw new Error("Image processing unavailable");
    context.drawImage(bitmap, 0, 0, canvas.width, canvas.height);
    return {
      name: file.name,
      dataUrl: canvas.toDataURL(maxDimension > 256 ? "image/webp" : "image/png", 0.9),
    };
  } finally {
    bitmap.close();
  }
}

export async function pickBrowserOtpUri(): Promise<string | null> {
  const file = await pickBrowserFile("image/*");
  if (!file) return null;
  if (file.size > 10 * 1024 * 1024) throw new Error("QR image exceeds 10 MiB");
  const bitmap = await createImageBitmap(file);
  try {
    if (!bitmap.width || !bitmap.height || bitmap.width * bitmap.height > 40_000_000)
      throw new Error("QR image dimensions exceed limit");
    const scale = Math.min(1, 2048 / Math.max(bitmap.width, bitmap.height));
    const canvas = document.createElement("canvas");
    canvas.width = Math.max(1, Math.round(bitmap.width * scale));
    canvas.height = Math.max(1, Math.round(bitmap.height * scale));
    const context = canvas.getContext("2d", { willReadFrequently: true });
    if (!context) throw new Error("QR image processing unavailable");
    context.drawImage(bitmap, 0, 0, canvas.width, canvas.height);
    const pixels = context.getImageData(0, 0, canvas.width, canvas.height);
    const { default: decode } = await import("jsqr");
    const result = decode(pixels.data, pixels.width, pixels.height);
    if (!result?.data.startsWith("otpauth://")) throw new Error("QR image has no OTP URI");
    return result.data;
  } finally {
    bitmap.close();
  }
}
