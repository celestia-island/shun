/// <reference types="vite/client" />
//
// Global JSX wiring for `jsx: preserve` + Vue: tsc keeps transform out of
// the way (vite's vue-jsx plugin owns it) while the type side resolves
// through vue's JSX tables — components accept children, intrinsics map
// to vue's DOM attributes.
import type { JSX as VueJSX } from "vue";

declare global {
  namespace JSX {
    // eslint-disable-next-line @typescript-eslint/no-empty-object-type
    interface Element extends VueJSX.Element {}
    // eslint-disable-next-line @typescript-eslint/no-empty-object-type
    interface ElementClass extends VueJSX.ElementClass {}
    interface ElementChildrenAttribute {
      children: {};
    }
    // eslint-disable-next-line @typescript-eslint/no-empty-object-type
    interface IntrinsicElements extends VueJSX.IntrinsicElements {}
  }
}

export {};
