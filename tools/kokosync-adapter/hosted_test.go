package main

import (
	"context"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"time"

	"github.com/katteni-kaihatu/kokosync/server/gateway"
)

func TestHostedBridgeAuthenticatesFragmentsAndCarriesLatest(t *testing.T) {
	server := gateway.New(".", "bridge-test")
	handler := server.Handler()
	web := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		user, password, ok := r.BasicAuth()
		if !ok || user != "test" || password != "test-only" {
			w.WriteHeader(http.StatusUnauthorized)
			return
		}
		handler.ServeHTTP(w, r)
	}))
	defer web.Close()
	ctx, cancel := context.WithTimeout(context.Background(), 15*time.Second)
	defer cancel()
	address := strings.Replace(web.URL, "http://", "ws://", 1) + "/ws"
	if _, _, err := connectRelay(ctx, address, "bridge-test", "denied", false); err == nil || !strings.Contains(err.Error(), "login rejected") {
		t.Fatalf("expected login rejection, got %v", err)
	}
	t.Setenv("KOKOSYNC_USERNAME", "test")
	t.Setenv("KOKOSYNC_PASSWORD", "test-only")
	a, closeA, err := connectRelay(ctx, address, "bridge-test", "a", false)
	if err != nil {
		t.Fatal(err)
	}
	defer closeA()
	defer a.Close()
	b, closeB, err := connectRelay(ctx, address, "bridge-test", "b", false)
	if err != nil {
		t.Fatal(err)
	}
	defer closeB()
	defer b.Close()
	go func() {
		for {
			select {
			case <-a.Messages():
			case <-a.Done():
				return
			}
		}
	}()
	payload := strings.Repeat("fragmented-content-", 20000)
	if _, err := a.SendTo(ctx, "b", "test", payload); err != nil {
		t.Fatal(err)
	}
	found := false
	for !found {
		select {
		case m := <-b.Messages():
			if m["type"] == "peer_message" {
				if m["value"] != payload {
					t.Fatal("fragment payload changed")
				}
				found = true
			}
		case <-ctx.Done():
			t.Fatal("fragmented message timed out")
		}
	}
	if err := a.Latest(ctx, "pose", 1, "moving"); err != nil {
		t.Fatal(err)
	}
	found = false
	for !found {
		select {
		case m := <-b.Messages():
			if m["type"] == "latest" {
				if m["value"] != "moving" {
					t.Fatal("latest payload changed")
				}
				found = true
			}
		case <-ctx.Done():
			t.Fatal("latest timed out")
		}
	}
	cancel()
	select {
	case <-b.Done():
	case <-time.After(time.Second):
		t.Fatal("bridge did not close with context")
	}
}

func TestHostedBridgeRejectsUnencryptedRemoteURLs(t *testing.T) {
	for _, address := range []string{"ws://45.77.130.146/ws", "wss://user:secret@example.test/ws", "http://example.test/ws"} {
		if _, err := openHostedBridge(context.Background(), address, nil); err == nil {
			t.Fatalf("accepted %s", address)
		}
	}
}

func TestAutomaticHostClaimAndGuestFallback(t *testing.T) {
	server := gateway.New(".", "auto-test")
	web := httptest.NewServer(server.Handler())
	defer web.Close()
	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()
	address := strings.Replace(web.URL, "http://", "ws://", 1) + "/ws"
	first, second := "auto", "auto"
	a, closeA, err := connectAutomatic(ctx, address, "auto-room", &first, false)
	if err != nil {
		t.Fatal(err)
	}
	defer closeA()
	defer a.Close()
	b, closeB, err := connectAutomatic(ctx, address, "auto-room", &second, false)
	if err != nil {
		t.Fatal(err)
	}
	defer closeB()
	defer b.Close()
	if first != "host" || !strings.HasPrefix(second, "guest-") {
		t.Fatalf("wrong automatic roles %q %q", first, second)
	}
	a.Close()
	if second == "host" {
		t.Fatal("guest must not change role when host leaves")
	}
}
