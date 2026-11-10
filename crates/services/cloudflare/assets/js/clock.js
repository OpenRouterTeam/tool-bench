((reference) => {
  const NativeDate = globalThis.Date;
  function Date(...arguments_) {
    if (!new.target) return new NativeDate(reference).toString();
    return Reflect.construct(NativeDate, arguments_.length ? arguments_ : [reference], new.target);
  }
  Object.defineProperties(Date, {
    length: { value: 7 },
    now: { value: () => reference, writable: true, configurable: true },
    parse: { value: NativeDate.parse, writable: true, configurable: true },
    UTC: { value: NativeDate.UTC, writable: true, configurable: true },
  });
  Date.prototype = NativeDate.prototype;
  Object.defineProperty(Date.prototype, "constructor", {
    value: Date, writable: true, configurable: true,
  });
  globalThis.Date = Date;
})(__business_now_ms);
delete globalThis.__business_now_ms;
