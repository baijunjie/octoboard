/** The side, in pixels, of the square a console's avatar is stored at: sharp enough for the largest
 * place it is drawn, with room for a high-density screen, and small enough to keep the stored
 * value short. */
const AVATAR_SIZE = 128;

/** Decodes an image file. `createImageBitmap` refuses an SVG, so that goes through an `<img>`. */
async function decode(file: File): Promise<{ source: CanvasImageSource; width: number; height: number; close: () => void }> {
  try {
    const bitmap = await createImageBitmap(file);
    return { source: bitmap, width: bitmap.width, height: bitmap.height, close: () => bitmap.close() };
  } catch {
    const url = URL.createObjectURL(file);
    try {
      const image = new Image();
      image.src = url;
      await image.decode();
      return { source: image, width: image.naturalWidth, height: image.naturalHeight, close: () => URL.revokeObjectURL(url) };
    } catch (error) {
      URL.revokeObjectURL(url);
      throw error;
    }
  }
}

/** Reads an image file into a `data:` URL of a `AVATAR_SIZE` square: the largest centred square of
 * the image, scaled to size. Encoded as WebP where the browser can, PNG otherwise. Rejects when
 * the file is not an image the browser can decode. */
export async function imageToAvatar(file: File): Promise<string> {
  const image = await decode(file);
  try {
    // An SVG with no intrinsic size decodes as 0 x 0.
    const side = Math.min(image.width, image.height);
    if (!side) throw new Error("image has no size");
    const canvas = document.createElement("canvas");
    canvas.width = AVATAR_SIZE;
    canvas.height = AVATAR_SIZE;
    const context = canvas.getContext("2d");
    if (!context) throw new Error("no 2d canvas");
    context.drawImage(
      image.source,
      (image.width - side) / 2,
      (image.height - side) / 2,
      side,
      side,
      0,
      0,
      AVATAR_SIZE,
      AVATAR_SIZE,
    );
    // A browser that cannot encode WebP answers with PNG, whatever was asked for.
    return canvas.toDataURL("image/webp", 0.9);
  } finally {
    image.close();
  }
}
