# Keyboard Shortcuts

This document lists all available keyboard shortcuts and TUI controls.

## Navigation

- **`j/k`** or **`↓/↑`** Navigate between tasks (down/up)
- **`J/K`** Navigate between sidebar entries (down/up)
- **`H`** Collapse the folder under the sidebar cursor
- **`L`** Expand the folder under the sidebar cursor
- **Mouse** Click on sidebar items or tasks to select them, scroll wheel to scroll

## Task Management

- **`Space`** or **`Enter`** Toggle the selected task. A pending task is completed, a completed or deleted one is restored.
- **`a`** Create new task. The text goes through Todoist Quick Add, so `Call Bob tomorrow 3pm #Work @phone p1` sets the due date, project, label and priority, and ` // text` at the end sets the description. With no `#project` in the text, the task goes to the project chosen in the dialog.
- **`c`** Create a subtask of the selected task. The dialog works like `a`, and the subtask goes in its parent's project.
- **`m`** Move the selected task under another task of its project, or back to the top level. Type to narrow the list, `↑/↓` to move, `Enter` to move the task.
- **`e`** Edit selected task
- **`d`** or **`Delete`** Delete selected task (with confirmation). A deleted task is restored instead.
- **`p`** Cycle task priority
- **`t`** Set task due date to today
- **`T`** Set task due date to tomorrow
- **`w`** Set task due date to next week (Monday)
- **`W`** Set task due date to next week end (Saturday)
- **`s`** Reschedule the selected task to any date. Todoist parses the text, so `fri 3pm`, `in 2 weeks`, `dec 24` and `every monday` all work, and `no date` removes the due date. A plain date on a repeating task ends the repeat; the dialog warns you when that applies.

## Project and Label Management

- **`A`** Create new project
- **`E`** Edit the selected project or label
- **`D`** Delete the selected project or label (with confirmation)

## System

- **`b`** Toggle sidebar visibility
- **`/`** Open task search dialog (search across all tasks)
- **`f`** Filter the Today, Tomorrow, Upcoming and label views to one project and its subprojects. Type to narrow the list, `↑/↓` to move, `Enter` to apply. Pick "All projects" to clear it. The filter stays on as you switch views, the task list title shows it, and project views ignore it.
- **`r`** Force sync with Todoist
- **`G`** Open the log dialog
- **`?`** or **`h`** Toggle help panel
- **`q`** Quit the application
- **`Esc`** Close the dialog on screen. With no dialog open, quit the application.
- **`Ctrl+C`** Quit application
- **`R`** Reload from the local cache. Available with `--debug` only.

## Dialogs

- **`Enter`** Confirm. In the search dialog, close it.
- **`Esc`** Cancel and close
- **`Tab`** / **`Shift+Tab`** In the task create and edit dialogs, move between the title, description and project fields. In the project creation dialog, cycle the parent project.
- **`Alt+Enter`** Start a new line in a task description. On macOS this needs the terminal to send Option as Meta (Terminal: Settings, Profiles, Keyboard, "Use Option as Meta key". iTerm2: Profiles, Keys, Left Option key "Esc+").
- **`Left/Right`** In the task project field, change the project
- **Type** In the search dialog, filter all tasks by content as you type
- **`Left/Right`** Move the cursor in the input field. `Up/Down` move between lines in a task description, and `Home/End` jump to the start and end of the line.
- **`Backspace/Delete`** Edit the input field

## Help Panel Scrolling

- **`↑/↓`** Scroll help content up/down
- **`Home/End`** Jump to top/bottom of help

## Interface Layout

### Layout Structure
- **Main Area**: Sidebar (views, projects, labels) | Tasks list (main area) - side by side

### Components
- **Sidebar (Left)**: Today, Tomorrow and Upcoming views, then a hierarchical display of all Todoist projects and labels
  - Configurable width via `sidebar_width` in config
  - Hidden and shown with `b`, and `sidebar_visible` sets the state at startup
  - Long project names are automatically truncated with ellipsis (…)
  - Parent-child relationships clearly shown
- **Tasks List (Right)**: Shows tasks for the currently selected sidebar entry
  - Takes remaining width after the sidebar
  - Displays task content, priority, labels, and status
- **Help Panel**: Modal overlay accessible with `?` or `h`
- **Sync Toast**: Sync progress is shown in a corner toast, it does not block the interface

### Task Display Features
Tasks are displayed with:
- **Status Icons**: ☐ (pending), ☒ (completed), ✗ (deleted)
- **Priority Flags**: a coloured flag before the content - ⚑ red (P1, urgent), ⚑ orange (P2, high), ⚑ blue (P3, medium), ⚐ white (P4, normal).
- **Label Badges**: Colored badges showing task labels
- **Subtasks**: Indented under their parent. A subtask whose parent isn't in the view (a subtask due today under a parent with no date, say) is listed on its own, followed by `↳` and its parent's name.
- **Task Content**: Truncated to fit the display width
- **Completion Visual**: Completed tasks appear dimmed
- **Interactive**: Press Space or Enter to toggle completion
