// Local test relay built from the same pinned KokoSync revision as the adapter.
package main

import (
	"context"
	"flag"
	"github.com/katteni-kaihatu/kokosync/server/gateway"
	"log"
	"net/http"
)

func main() {
	address := flag.String("quic", "127.0.0.1:48001", "local QUIC endpoint")
	httpAddress := flag.String("http", "127.0.0.1:48000", "local HTTP endpoint")
	flag.Parse()
	if err := gateway.ValidateHTTPAddr(*httpAddress); err != nil {
		log.Fatal(err)
	}
	server := gateway.New(".", "kokoverse-local-development")
	ctx := context.Background()
	go server.Core.RunRoomCleanup(ctx)
	go func() {
		if err := server.ServeQUIC(ctx, *address); err != nil {
			log.Fatal(err)
		}
	}()
	log.Printf("Local KokoSync relay: QUIC %s, HTTP %s", *address, *httpAddress)
	log.Fatal(http.ListenAndServe(*httpAddress, server.Handler()))
}
