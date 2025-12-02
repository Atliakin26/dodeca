# Smart Live Reload Plan

## The Vision

Instead of full page reloads, send minimal DOM patches over WebSocket.
Client becomes a dumb patch applier. Server does all the diffing.

## Why This Works

Salsa caches the previous render. When content changes:

1. **Before**: Old HTML is in Salsa's cache
2. **After**: New HTML is computed
3. **Diff**: Server compares old DOM → new DOM
4. **Patch**: Send only the operations
5. **Apply**: Client applies patches, no diffing needed

## Phase 1: CSS Hot Reload (Quick Win)

### Server Changes
- Detect when only CSS changed (not HTML)
- Send targeted message: `{"type": "css", "path": "/css/style.css", "hash": "abc123"}`

### Client Changes
```js
if (msg.type === "css") {
  const link = document.querySelector(`link[href*="${msg.path}"]`);
  const clone = link.cloneNode();
  clone.href = `${msg.path.replace(/\.[a-f0-9]+\.css/, `.${msg.hash}.css`)}`;
  clone.onload = () => link.remove();
  link.after(clone);
}
```

No FOUC, no full page reload.

## Phase 2: HTML DOM Patching

### Server-Side DOM Diffing

Parse both old and new HTML into DOM trees, compute minimal edit script.

#### Patch Operations
```rust
enum DomPatch {
    Replace { selector: String, html: String },
    InsertBefore { selector: String, html: String },
    InsertAfter { selector: String, html: String },
    Remove { selector: String },
    SetAttribute { selector: String, name: String, value: String },
    RemoveAttribute { selector: String, name: String },
    SetText { selector: String, text: String },
}
```

#### Wire Format
```json
{
  "type": "dom",
  "patches": [
    {"op": "replace", "sel": "#content > p:nth-child(3)", "html": "<p>New text</p>"},
    {"op": "set-attr", "sel": "article", "name": "data-updated", "value": "true"}
  ]
}
```

### Client-Side Patch Applier
```js
const ops = {
  replace: (sel, html) => {
    const el = document.querySelector(sel);
    el.outerHTML = html;
  },
  remove: (sel) => {
    document.querySelector(sel)?.remove();
  },
  "insert-before": (sel, html) => {
    const el = document.querySelector(sel);
    el.insertAdjacentHTML("beforebegin", html);
  },
  // ... etc
};

ws.onmessage = (e) => {
  const msg = JSON.parse(e.data);
  if (msg.type === "dom") {
    for (const p of msg.patches) {
      ops[p.op](p.sel, p.html || p.value);
    }
  }
};
```

## DOM Diffing Strategies

### Option A: Tree Edit Distance (Precise but Complex)
- Academic algorithm (Zhang-Shasha, RTED)
- Computes minimal insert/delete/replace operations
- O(n²) to O(n³) complexity
- Overkill for most cases

### Option B: Keyed Matching (What morphdom/idiomorph do)
- Match nodes by `id` or `data-key` attributes
- Unmatched nodes: insert/remove
- Matched nodes: recurse and diff children
- Much faster, good enough for real use

### Option C: Block-Level Diffing (Simpler)
- Treat major sections as units (header, article, footer)
- Hash each section's content
- Only diff sections that changed
- Very fast, may miss fine-grained updates

### Recommendation
Start with **Option C** (block-level), graduate to **Option B** (keyed) if needed.

Block-level is perfect for content sites:
- Nav changes? Replace nav.
- Article changes? Replace article.
- Footer same? Don't touch it.

## Addressing Elements

How to identify elements for patching:

### Option 1: CSS Selectors
- `#content > p:nth-child(3)`
- Fragile if structure changes between old/new

### Option 2: Stable IDs/Keys
- Add `data-dd-id="xyz"` during render
- Server tracks these across renders
- More robust but requires render-time work

### Option 3: Path-based
- `/html/body/main/article/p[2]`
- XPath-style, stable within a single render

## Preserving State

The beauty of patching: these are preserved automatically:
- Scroll position
- Focus state
- Form inputs (if not in patched region)
- CSS animations
- JavaScript component state

## Future Ideas

### Salsa-Aware Diffing
Since Salsa knows *what* changed (which query was invalidated), we could:
- Skip diffing unchanged sections entirely
- Know that "only the code block in section 2 changed"
- Ultra-targeted patches

### Binary Patches
For large pages, JSON patches might be verbose. Could use:
- MessagePack
- Custom binary format
- Or just gzip the WebSocket messages

## Implementation Order

1. [ ] CSS hot reload (Phase 1)
2. [ ] Block-level HTML diffing (Phase 2a)
3. [ ] Keyed node matching (Phase 2b)
4. [ ] Salsa-aware targeted diffing (Phase 3)
5. [ ] Binary/compressed patches (optimization)
