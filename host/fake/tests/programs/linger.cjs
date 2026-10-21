// Preloaded into a server (`node --require linger.cjs server.js`) so that it does not leave when its
// stdin closes: the end of the input is what a well-behaved stdio server takes as the end, and the
// program that started it must end it by other means (a signal) when the terminal is gone. A pending
// timer keeps the process; only a signal ends it.
setInterval(() => {}, 1000);
