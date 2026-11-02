// How long a preview frame has to report ready before the host shows its error.
const READY_TIMEOUT_MS = 20000;

interface PreviewFrameOptions {
  host: Ref<HTMLElement | null>;
  frame: Ref<HTMLIFrameElement | null>;
  /** The frame's address; a new one starts the wait for readiness over. */
  source: Ref<string>;
  /** How far outside the viewport the host may be and still mount its frame. */
  rootMargin: string;
  /** The host's size before the first measurement. */
  initialSize: { width: number; height: number };
  /** A message from the frame on this origin, as posted. Reports readiness through `markReady` and `markFailed`. */
  onMessage: (data: unknown) => void;
}

/** The lifecycle a host of an iframe-embedded preview needs: the frame mounts once its host nears the viewport, the
 * host's size is tracked, and the frame has a bounded time to report ready. Only messages from that frame's window on
 * the page's own origin reach `onMessage`. */
export function usePreviewFrame(options: PreviewFrameOptions) {
  const { host, frame, source, rootMargin, onMessage } = options;
  const width = ref(options.initialSize.width);
  const height = ref(options.initialSize.height);
  const visible = ref(false);
  const ready = ref(false);
  const failed = ref(false);
  const attempt = ref(0);
  let observer: IntersectionObserver | undefined;
  let dimensions: ResizeObserver | undefined;
  let readinessTimer: ReturnType<typeof setTimeout> | undefined;

  function stopWaiting() {
    clearTimeout(readinessTimer);
  }
  function markReady() {
    stopWaiting();
    ready.value = true;
    failed.value = false;
  }
  function markFailed() {
    stopWaiting();
    failed.value = true;
  }
  /** Loads the frame again, as a new `attempt` keys it. */
  function retry() {
    attempt.value++;
  }

  watch([visible, source, attempt], () => {
    stopWaiting();
    ready.value = false;
    failed.value = false;
    if (visible.value) readinessTimer = setTimeout(markFailed, READY_TIMEOUT_MS);
  });

  function receive(event: MessageEvent) {
    if (
      event.source !== frame.value?.contentWindow ||
      event.origin !== window.location.origin
    )
      return;
    onMessage(event.data);
  }
  onMounted(() => {
    if (!host.value) return;
    dimensions = new ResizeObserver(([entry]) => {
      if (!entry) return;
      width.value = entry.contentRect.width;
      height.value = entry.contentRect.height;
    });
    dimensions.observe(host.value);
    observer = new IntersectionObserver(
      ([entry]) => {
        if (entry?.isIntersecting) {
          visible.value = true;
          observer?.disconnect();
        }
      },
      { rootMargin },
    );
    observer.observe(host.value);
    window.addEventListener("message", receive);
  });
  onBeforeUnmount(() => {
    stopWaiting();
    dimensions?.disconnect();
    observer?.disconnect();
    window.removeEventListener("message", receive);
  });

  return { width, height, visible, ready, failed, attempt, markReady, markFailed, retry };
}
