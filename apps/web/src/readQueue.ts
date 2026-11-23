// Hold the slot until the response body has been consumed, not just headers.
export function createReadQueue(gapMs = 200) {
  let tail: Promise<unknown> = Promise.resolve();
  return function enqueue<T>(read: () => Promise<T>): Promise<T> {
    const result = tail.then(read);
    tail = result.catch(() => undefined).then(() => new Promise(resolve => setTimeout(resolve, gapMs)));
    return result;
  };
}
