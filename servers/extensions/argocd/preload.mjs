import { checkedNativeFetch } from "./native-fetch.mjs";

globalThis.fetch = checkedNativeFetch(globalThis.fetch);
