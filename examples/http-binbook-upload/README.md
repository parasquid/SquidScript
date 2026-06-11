# HTTP BinBook Upload

Starts the device AP, registers `service.http.start("file-upload", ...)`, and
publishes uploaded `.binbook` files into `content.binbook.list("books")`.

After launching the app on firmware, join the `SquidScript-X4` AP and upload:

```sh
curl -T book.binbook http://192.168.4.1/upload/book.binbook
```

The SD card is device-owned. The host OS does not mount it directly; uploads go
through the firmware HTTP route.
