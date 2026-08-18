import { clsx, type ClassValue } from 'clsx'
import { twMerge } from 'tailwind-merge'

// The shadcn-svelte CLI expects this module at the `utils` alias in components.json, and
// generated components import both `cn` and the type helpers below from it. Keep the
// exports as they are; `shadcn-svelte add` will not recreate them.

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs))
}

export type WithoutChild<T> = T extends { child?: any } ? Omit<T, 'child'> : T
export type WithoutChildren<T> = T extends { children?: any } ? Omit<T, 'children'> : T
export type WithoutChildrenOrChild<T> = WithoutChildren<WithoutChild<T>>
export type WithElementRef<T, U extends HTMLElement = HTMLElement> = T & { ref?: U | null }
