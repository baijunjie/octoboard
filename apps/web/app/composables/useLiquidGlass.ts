import type { Ref } from "vue";
import {
  liquidGlassMap,
  type LiquidGlassMapOptions,
} from "../utils/liquidGlassMap";

/**
 * Refract the backdrop through a rounded rim with local blur behind navigation labels;
 * lightly blurred RGB samples use slightly different offsets for chromatic dispersion at the edge.
 * Browsers without SVG backdrop support keep the CSS blur fallback.
 */
interface LiquidGlassOptions extends Partial<LiquidGlassMapOptions> {
  rimBlur?: number;
  saturate?: number;
  dispersion?: number;
}

const SVG_NS = "http://www.w3.org/2000/svg";
let filterSeq = 0;

function supportsBackdropSvgFilter() {
  // Safari reports support for url() but does not render it. iOS Chromium brands also use WebKit.
  return (
    /Chrome\/\d+/.test(navigator.userAgent) &&
    CSS.supports("backdrop-filter", "blur(1px)")
  );
}

function mapToDataUrl(width: number, height: number, data: Uint8ClampedArray) {
  const canvas = document.createElement("canvas");
  canvas.width = width;
  canvas.height = height;
  const context = canvas.getContext("2d")!;
  const image = context.createImageData(width, height);
  image.data.set(data);
  context.putImageData(image, 0, 0);
  return canvas.toDataURL();
}

function channelMatrix(channel: 0 | 1 | 2) {
  const rows = [
    [0, 0, 0, 0, 0],
    [0, 0, 0, 0, 0],
    [0, 0, 0, 0, 0],
    [0, 0, 0, 1, 0],
  ];
  rows[channel]![channel] = 1;
  return rows.flat().join(" ");
}

export function useLiquidGlass(
  target: Ref<HTMLElement | null>,
  options: LiquidGlassOptions = {},
) {
  const settings: Required<LiquidGlassOptions> = {
    bezel: 24,
    edgeMagnification: 10,
    falloff: 2,
    rimStart: 0.75,
    rimEnd: 1.25,
    rimBlur: 1.5,
    saturate: 1.5,
    dispersion: 0.03,
    ...options,
  };
  // Larger dispersion can fold the blue sampling map back on itself at the edge.
  const dispersion = Math.min(
    settings.dispersion,
    0.9 / Math.max(settings.edgeMagnification - 1, 1e-6),
  );

  let svg: SVGSVGElement | null = null;
  let observer: ResizeObserver | null = null;
  let lastSize = "";
  let detach: (() => void) | undefined;

  function render(el: HTMLElement) {
    const width = el.offsetWidth;
    const height = el.offsetHeight;
    const bounds = el.getBoundingClientRect();
    const labels = Array.from(el.querySelectorAll(".brand, .desktop-nav a"));
    const regions = labels.map((label) => {
      const rect = label.getBoundingClientRect();
      return {
        x: (rect.left - bounds.left) * width / bounds.width - 4,
        y: (rect.top - bounds.top) * height / bounds.height - 4,
        width: rect.width * width / bounds.width + 8,
        height: rect.height * height / bounds.height + 8,
      };
    });
    const size = `${width}x${height}:${JSON.stringify(regions)}`;
    if (!width || !height || size === lastSize || !svg) return;
    lastSize = size;

    const radius =
      Number.parseFloat(getComputedStyle(el).borderTopLeftRadius) || 0;
    const map = liquidGlassMap(width, height, radius, settings);
    const filter = svg.querySelector("filter")!;
    filter.setAttribute("width", String(width));
    filter.setAttribute("height", String(height));
    const feImage = filter.querySelector('feImage[result="map"]')!;
    feImage.setAttribute("width", String(width));
    feImage.setAttribute("height", String(height));
    feImage.setAttribute("href", mapToDataUrl(width, height, map.data));
    // Each label gets its own blur patch; the gaps continue to sample the clear backdrop.
    const maskImage = filter.querySelector('feImage[result="labelMask"]')!;
    maskImage.setAttribute("width", String(width));
    maskImage.setAttribute("height", String(height));
    const mask = `<svg xmlns="${SVG_NS}" width="${width}" height="${height}">${regions
      .filter((region) => region.width > 8 && region.height > 8)
      .map((region) => `<rect x="${region.x}" y="${region.y}" width="${region.width}" height="${region.height}" rx="${Math.min(region.width, region.height) / 2}" fill="white"/>`)
      .join("")}</svg>`;
    maskImage.setAttribute("href", `data:image/svg+xml,${encodeURIComponent(mask)}`);
    const factors = { R: 1 - dispersion, G: 1, B: 1 + dispersion };
    for (const [channel, factor] of Object.entries(factors)) {
      filter
        .querySelector(`feDisplacementMap[data-channel="${channel}"]`)!
        .setAttribute("scale", String(map.scale * factor));
    }
  }

  function attach(el: HTMLElement) {
    const id = `liquid-glass-${++filterSeq}`;
    svg = document.createElementNS(SVG_NS, "svg");
    svg.setAttribute("aria-hidden", "true");
    svg.setAttribute("width", "0");
    svg.setAttribute("height", "0");
    svg.style.position = "absolute";
    // The map's blue channel blends the softened rim into the clear center before displacement.
    svg.innerHTML = `
      <filter id="${id}" x="0" y="0" filterUnits="userSpaceOnUse" color-interpolation-filters="sRGB">
        <feImage x="0" y="0" preserveAspectRatio="none" result="map" />
        <feImage x="0" y="0" preserveAspectRatio="none" result="labelMask" />
        <feGaussianBlur in="labelMask" stdDeviation="2" result="softLabelMask" />
        <feGaussianBlur in="SourceGraphic" stdDeviation="12" result="labelBlur" />
        <feComposite in="labelBlur" in2="softLabelMask" operator="in" result="labelsWeighted" />
        <feComposite in="SourceGraphic" in2="softLabelMask" operator="out" result="gapsWeighted" />
        <feComposite in="labelsWeighted" in2="gapsWeighted" operator="arithmetic" k2="1" k3="1" result="center" />
        <feColorMatrix in="map" type="matrix" values="0 0 0 0 0  0 0 0 0 0  0 0 0 0 0  0 0 1 0 0" result="wRim" />
        <feColorMatrix in="map" type="matrix" values="0 0 0 0 0  0 0 0 0 0  0 0 0 0 0  0 0 -1 0 1" result="wCenter" />
        <feGaussianBlur in="SourceGraphic" stdDeviation="${settings.rimBlur}" result="rim" />
        <feComposite in="rim" in2="wRim" operator="in" result="rimWeighted" />
        <feComposite in="center" in2="wCenter" operator="in" result="centerWeighted" />
        <feComposite in="rimWeighted" in2="centerWeighted" operator="arithmetic" k2="1" k3="1" result="source" />
        <feDisplacementMap data-channel="R" in="source" in2="map" xChannelSelector="R" yChannelSelector="G" result="shiftedR" />
        <feDisplacementMap data-channel="G" in="source" in2="map" xChannelSelector="R" yChannelSelector="G" result="shiftedG" />
        <feDisplacementMap data-channel="B" in="source" in2="map" xChannelSelector="R" yChannelSelector="G" result="shiftedB" />
        <feColorMatrix in="shiftedR" type="matrix" values="${channelMatrix(0)}" result="onlyR" />
        <feColorMatrix in="shiftedG" type="matrix" values="${channelMatrix(1)}" result="onlyG" />
        <feColorMatrix in="shiftedB" type="matrix" values="${channelMatrix(2)}" result="onlyB" />
        <feComposite in="onlyR" in2="onlyG" operator="arithmetic" k2="1" k3="1" result="onlyRG" />
        <feComposite in="onlyRG" in2="onlyB" operator="arithmetic" k2="1" k3="1" result="refracted" />
        <feColorMatrix in="refracted" type="saturate" values="${settings.saturate}" />
      </filter>`;
    document.body.appendChild(svg);

    render(el);
    el.style.backdropFilter = `url(#${id})`;
    el.classList.add("is-refracting");
    // Padding changes affect the displacement map, so observe the border box.
    observer = new ResizeObserver(() => render(el));
    observer.observe(el, { box: "border-box" });
    for (const label of el.querySelectorAll(".brand, .desktop-nav a")) {
      observer.observe(label, { box: "border-box" });
    }
  }

  function clear(el: HTMLElement) {
    observer?.disconnect();
    observer = null;
    svg?.remove();
    svg = null;
    lastSize = "";
    el.style.backdropFilter = "";
    el.classList.remove("is-refracting");
  }

  onMounted(() => {
    const el = target.value;
    if (!el || !supportsBackdropSvgFilter() || !("ResizeObserver" in window))
      return;
    const motion = matchMedia("(prefers-reduced-motion: reduce)");
    const synchronize = () => {
      if (motion.matches) clear(el);
      else if (!svg) attach(el);
    };
    synchronize();
    motion.addEventListener("change", synchronize);
    detach = () => {
      motion.removeEventListener("change", synchronize);
      clear(el);
    };
  });

  onBeforeUnmount(() => {
    detach?.();
    detach = undefined;
  });
}
