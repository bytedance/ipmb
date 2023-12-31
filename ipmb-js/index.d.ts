/* tslint:disable */
/* eslint-disable */

/* auto-generated by NAPI-RS */

export const enum SelectorMode {
  Unicast = 0,
  Multicast = 1
}
export interface Selector {
  labelOp: LabelOp
  mode: SelectorMode
  ttl: number
}
export interface Options {
  identifier: string
  label: Array<string>
  token: string
  controllerAffinity: boolean
}
export interface BytesMessage {
  format: number
  data: Buffer
}
export function join(options: Options, timeout?: number | undefined | null): { sender: Sender, receiver: Receiver }
export class LabelOp {
  constructor(v: boolean | string)
  not(): void
  and(right: LabelOp): void
  or(right: LabelOp): void
  toString(): string
}
export class Object {
  value(): number
}
export class MemoryRegion {
  map(offset: number, size: number): Buffer
}
export class Sender {
  send(selector: Selector, bytesMessage: BytesMessage, buffers: Array<Buffer>): void
}
export class Receiver {
  recv(timeout?: number | undefined | null): Promise<{ bytesMessage: BytesMessage, objects: Array<Object>, memoryRegions: Array<MemoryRegion> }>
  close(): void
}
