import {
  Archive,
  Cpu,
  Download,
  FileCog,
  FileText,
  Focus,
  FolderTree,
  GitBranch,
  GitCompare,
  Inbox,
  Funnel,
  type LucideIcon,
  MessageSquareWarning,
  PanelsTopLeft,
  Plug,
  Presentation,
  Rows3,
  Settings,
  SquareTerminal,
  Terminal,
} from "lucide-react";

/** The groups in the order the gallery lists them, each with the icon its heading carries so the
 * groups can be told apart at a glance. The order follows the way the app is met: how it comes up,
 * holds its connection and looks with nothing in it yet, then the sidebar and the views reached from
 * it, then the terminal and the right pane that fill the content panel and the viewer they open,
 * then what opens over them, and last the window itself. */
export const GROUPS = [
  { title: "Startup and connection", icon: Plug },
  { title: "Welcome", icon: Inbox },
  { title: "Consoles", icon: Rows3 },
  { title: "Projects", icon: FolderTree },
  { title: "Sidebar sessions", icon: Rows3 },
  { title: "Branch badges", icon: GitBranch },
  { title: "Project filter", icon: Funnel },
  { title: "Focus mode", icon: Focus },
  { title: "Archive view", icon: Archive },
  { title: "Processes", icon: Cpu },
  { title: "Terminal", icon: SquareTerminal },
  { title: "Terminal panel", icon: Terminal },
  { title: "Report panel", icon: Presentation },
  { title: "Project pane: files", icon: FolderTree },
  { title: "Project pane: Git", icon: GitCompare },
  { title: "File viewer", icon: FileText },
  { title: "Trust prompts and toasts", icon: MessageSquareWarning },
  { title: "Configuration", icon: FileCog },
  { title: "Settings", icon: Settings },
  { title: "Updates", icon: Download },
  { title: "Window layout", icon: PanelsTopLeft },
  { title: "Component colors", icon: FileText },
] as const satisfies readonly { title: string; icon: LucideIcon }[];

/** What a scenario's `group` names. */
export type GroupTitle = (typeof GROUPS)[number]["title"];

/** Every function lists normal states first, then deliberate failures and conflicts. */
export const CATEGORIES = [
  { id: "normal", label: "Normal", description: "Expected UI, including empty, loading and constrained views." },
  { id: "exceptional", label: "Exceptional", description: "Deliberate failures, validation errors, refusals and conflicts." },
] as const;
