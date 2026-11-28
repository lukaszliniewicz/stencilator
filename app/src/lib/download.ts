/** Hands bytes to the browser as a download. */
export function save(bytes: Uint8Array | Blob, name: string, type: string): void {
  const blob =
    bytes instanceof Blob
      ? bytes.type === type
        ? bytes
        : bytes.slice(0, bytes.size, type)
      : new Blob([bytes.slice()], { type });
  const url = URL.createObjectURL(blob);
  const link = document.createElement('a');
  link.href = url;
  link.download = name;
  link.click();
  // Give the browser time to start reading before the address is withdrawn.
  setTimeout(() => {
    URL.revokeObjectURL(url);
  }, 60_000);
}
