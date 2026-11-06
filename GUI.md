# Desktop GUI

`apic-gui` works on the same `.apic` projects and the same JSON files as the CLI. Both are thin layers over the shared `apic-core` crate, so a contract edited in one reads the same in the other.

```bash
apic-gui
```

## Projects

- **Open, New, Import.** With no project open, the welcome screen offers New project, Open project and an Import menu. The same actions live in the project dropdown in the app bar.
  - **Open** only accepts a folder that already holds `.apic/config.toml`, and says so in a popup otherwise.
  - **New** sets up `.apic` in a folder and opens it.
  - **Import** reads a Postman collection into the open project. With no project open, it asks for a folder, sets it up as a new project and imports into it.
- **Remembered.** The last project reopens on the next launch, and so does the last contract or template you had open in each project, as long as that file is still a valid contract inside that project.
- **Theme.** The sun and moon icon on the right of the app bar switches between light and dark, and the choice is saved.

## Contracts

- **Browse.** The sidebar lists every contract as a folder tree with its method badge. The search icon filters the tree, and `+` starts a new request. Hover a row for its `⋮` menu, or right-click it: a folder offers New request here and Delete folder, and a contract offers Delete. Templates live in the collapsible Templates section at the bottom.
- **Read and edit.** Selecting a contract shows its name and description, the method and URL, and the Headers, Query and Request sections, with the responses on the right. The pencil enters edit mode, and the disk saves. A dot next to the disk means unsaved edits.
  - Headers, query params and response headers are grid tables. While editing, the trash icon removes a row and typing into the empty last row adds one.
  - Variables in the URL and in values, `$name`, `${name}`, `{{name}}` and `{name}`, are highlighted in the variable colour. With an environment selected, a name it does not define is drawn in the warning colour instead, unless the path-param popup has a value for it.
  - Each body has a JSON / Multipart dropdown while editing. Multipart bodies are a parts table with Text or File parts.
  - Responses are picked from the dropdown on the right, coloured by status class. "+ New example" adds one with a code and a name.
- **Repair.** An invalid contract opens in a raw JSON editor instead. Once the JSON is valid, the GUI switches back to the structured view.
- **Asks before losing work.** Cancel, opening another contract, opening or creating a project, and closing the window all ask what to do with unsaved edits: Save, Discard or Keep editing. Every delete and discard also asks first.

## Sending

The Send button at the right end of the URL bar, or Ctrl+Enter (Cmd+Enter on macOS), sends the open contract as it is on screen, unsaved edits included, and never writes the file. While the request is in flight the button reads Cancel, and Cancel drops the reply. A request gives up after 30 seconds.

The environment dropdown in the app bar, next to the branch dropdown, lists No environment and every environment in `.apic/env`. The choice is remembered per project. Every send reads the environment from disk again, so an edit to `dev.local.toml` applies on the next send. [SENDING.md](SENDING.md) explains environment files.

- A `{name}` path param the environment does not fill opens a popup that asks for its value before sending. The values are kept for the session and prefilled next time.
- A variable the environment does not define is sent as written, and the status line names it.
- The right pane switches between Examples, the documented responses, and Live, the real one. Send switches to Live, which shows the status coloured like the examples, the time and the size, the body under Response and the reply headers under Headers. A JSON body is pretty-printed and highlighted, other text shows as it is, and a binary body shows its size. Only the first 2 MB of a body is shown.

## Git

When the project sits inside a git repository, the app bar shows a branch dropdown and the sidebar can switch from Contracts to Changes. Everything shells out to your own `git`, so hooks, credential helpers and config behave as they do in a terminal.

- **Branches.** The dropdown lists local branches. Picking one checks it out, "+ New branch" creates one, and right-clicking a branch offers Delete branch behind a confirmation. Deleting is never forced. Switching is refused while a contract has unsaved edits, and after a switch the open contract is reloaded from the new branch, or closed if it does not exist there.
- **Changes.** Every changed file appears once with a checkbox, nested by folder, and the commit contains exactly the ticked files. Conflicts and files outside the project have their own groups. Right-click a file or folder to Unstage or Discard changes.
- **Commit.** The Commit section at the bottom of the sidebar holds the message, an Amend checkbox and the Commit button. Amend loads the last message, and amending a commit that is already on its upstream warns and asks first, since it needs a force push.
- **Diffs.** Selecting a file shows its diff inline, the way VS Code does: old and new line numbers, removed lines on a red background, added lines on green, and the changed part of an edited line in a stronger tint. For a changed contract, "Show field changes" switches to a field-level summary like `name user-login -> login-v2`.
- **Conflicts.** A conflicted file opens inline, the way VS Code shows it: each conflict has Accept Current, Accept Incoming and Accept Both links above its two sides, Current tinted green and Incoming blue. Opening a conflicted contract from the Contracts list takes you to the same resolver under Changes. A decided block shows the text it will keep, with Undo to choose again. Resolve writes the file and stages it once every block is decided.

Not included: merge, pull, fetch, rebase, stash and remote branches. Those stay in your terminal.
