// The agent terminal's input stands in for xterm's in the App tests: the controller mounts it and focuses it when
// asked to. They mock `./terminal/TerminalController` with this module.
export class TerminalController {
  input?: HTMLTextAreaElement;
  constructor() {
    return new Proxy(this, { get: (target, key) => (key === "then" ? undefined : key in target ? Reflect.get(target, key) : () => {}) });
  }
  mount(container: HTMLElement) {
    this.input = container.appendChild(document.createElement("textarea"));
    this.input.setAttribute("aria-label", "Agent terminal");
  }
  focus() {
    this.input?.focus();
  }
}
