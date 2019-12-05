# Sidecar HTTP Proxy

This is a super simple sidecar HTTP proxy written in Rust. 
It is designed to be used as a sidecar for rewriting requests so that you can have an ingress controller agnostic Kubernetes Pod where the webapp in the main pod is not able to modify its context root.

```
Usage: sidecar-http-proxy [options]

Options:
    -h, --help          print this help menu and exit
    -V, --version       print the version and exit
    -p, --port PORT     the port to listen for requests on (default: 8080)
    -t, --target-url URL
                        the target base URL to proxy
    -s, --source-path PATH
                        the source path to remove from requests before
                        forwarding to the target (default: /)
    -c, --cache-control VALUE
                        the cache control header to inject if none is provided


Proxies requests to a remote service (with optional path prefix stripping)
```

## Example usage in a pod

The use case is where the application (I'm looking at you Apache Flink) does not support configuring the context path to serve from.
Normally you would just use a rewrite rule in your ingress controller, but of course all the ingress controllers have different syntax for rewrite rules (I'm looking at you traefik and nginx) thus if you want a simple ingress agnostic chart you need to have the URLs rewritten for you.
Enter this sidecar container, which runs in the same pod and can thus access `localhost` ports and perform the rewrite for you.

Adding the following sidecar conatiner will serve http://127.0.0.1:8081/ at http://127.0.0.1:8080/context-root/ allowing your ingress rule to just point at that path without requiring any rewrites.

```
      containers:
        {{- if .Values.ingress.enabled }}
        - name: {{ .Chart.Name }}-proxy
          image: "stephenc/sidecar-http-proxy:0.1.1"
          imagePullPolicy: IfNotPresent
          command: ["/sidecar-http-proxy", "-p", "8080", "-s", "/context-root", "-t", "http://127.0.0.1:8081"]
          ports:
            - containerPort: 8080
              name: proxy
          livenessProbe:
            tcpSocket:
              port: 8080
            initialDelaySeconds: 5
            periodSeconds: 60
        {{- end }}
        - name: {{ .Chart.Name }}-app
          ...
```
