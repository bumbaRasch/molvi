// Signal store (plan Task 14 Step 3). A plain typed holder — re-render goes
// through the language listener + direct DOM (no sub() reactivity hub was ever
// wired), so this carries get/set only.
export class Store<T extends object> {
  constructor(private state: T) {}
  get(): Readonly<T> { return this.state; }
  set(patch: Partial<T>) { Object.assign(this.state, patch); }
}
