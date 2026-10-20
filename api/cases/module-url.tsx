// Keep the compiler helper that resolves import.meta.url in delivered apps.
import { Text, mount } from "nsh";
const moduleURL = new URL(import.meta.url);
mount(() => <Text>{`Module URL: ${["asset:", "file:"].includes(moduleURL.protocol) && moduleURL.pathname.endsWith("module-url.tsx")}`}</Text>);
