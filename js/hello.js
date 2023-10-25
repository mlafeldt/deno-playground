if (typeof setTimeout !== "undefined") {
  setTimeout(() => console.log("Hello"), 500)
} else {
  Deno.core.print("Hello\n")
}
