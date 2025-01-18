FROM debian:stable-slim AS build

RUN apt-get update && apt-get install -y gcc
WORKDIR /app
COPY hello.c .
RUN gcc -g -O0 -o hello hello.c

FROM scratch AS export-stage
COPY --from=build /app/hello /hello

FROM debian:stable-slim
RUN apt-get update && apt-get install -y gdb gdbserver openssh-server
COPY --from=build /app/hello /usr/local/bin/hello
COPY start.sh /start.sh
RUN chmod +x /start.sh

EXPOSE 22 2159

CMD ["/start.sh"]
