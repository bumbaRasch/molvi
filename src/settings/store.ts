// Signal store (plan Task 14 Step 3 — pasted verbatim per R5).
// The single reactivity hub: section UIs sub() to rerender on set().
type Listener = () => void;
export class Store<T extends object> {
  private ls = new Set<Listener>();
  constructor(private state: T) {}
  get(): Readonly<T> { return this.state; }
  set(patch: Partial<T>) { Object.assign(this.state, patch); this.ls.forEach(l => l()); }
  sub(l: Listener): () => void { this.ls.add(l); return () => this.ls.delete(l); }
}
