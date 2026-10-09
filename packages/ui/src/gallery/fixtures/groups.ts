import {
  AppWindow,
  Archive,
  FileText,
  Focus,
  FolderTree,
  GitBranch,
  GitCompare,
  Inbox,
  ListFilter,
  type LucideIcon,
  MessageSquareWarning,
  PanelsTopLeft,
  Plug,
  Presentation,
  Rows3,
  Settings,
  SquareTerminal,
} from "lucide-react";

/** The groups in the order the gallery lists them, each with the icon its heading carries so the
 * groups can be told apart at a glance. */
export const GROUPS = [
  { title: "Empty", icon: Inbox },
  { title: "Sidebar sessions", icon: Rows3 },
  { title: "Focus mode", icon: Focus },
  { title: "Branch badges", icon: GitBranch },
  { title: "Project filter", icon: ListFilter },
  { title: "Startup and connection", icon: Plug },
  { title: "Terminal", icon: SquareTerminal },
  { title: "Trust prompts and toasts", icon: MessageSquareWarning },
  { title: "Archive view", icon: Archive },
  { title: "Report panel", icon: Presentation },
  { title: "Project pane: files", icon: FolderTree },
  { title: "Project pane: Git", icon: GitCompare },
  { title: "Settings", icon: Settings },
  { title: "Dialogs", icon: AppWindow },
  { title: "File viewer", icon: FileText },
  { title: "Window layout", icon: PanelsTopLeft },
] as const satisfies readonly { title: string; icon: LucideIcon }[];

/** What a scenario's `group` names. */
export type GroupTitle = (typeof GROUPS)[number]["title"];
