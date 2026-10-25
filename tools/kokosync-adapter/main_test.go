package main

import (
	"bytes"
	"context"
	"encoding/json"
	"strings"
	"testing"
	"time"

	"github.com/katteni-kaihatu/kokosync/protocol"
)

func TestFullTransformLatestFitsDatagramAndRoundTrips(t *testing.T) {
	target := map[string]any{"instance_id": "af421058-ae92-4b71-9561-6efb5c1bbfe4", "local_id": 1}
	point := []float64{991.436732935727, -751.336742186233, 927.435232465983}
	packet := map[string]any{
		"protocol_version": 3, "session_id": strings.Repeat("s", 80), "authority_epoch": "afe42105-8ae9-4b71-9561-6efb5c1bbfe4", "sequence": "18446744073709551615",
		"body": map[string]any{"kind": "pose", "name": strings.Repeat("界", 40), "avatar_texture": "1fe4136475276c4c82c867255ec7c7697533253c924bc0928804b4ba3324be2a",
			"pose":    map[string]any{"position": point, "yaw": 0.726437298573, "pitch": 0.258463829573},
			"preview": map[string]any{"target": target, "position": point, "rotation": []float64{0.324723954231, 0.347529863571, 0.736457238951, 0.477839279521}, "scale": []float64{1.34759827591, 1.34759827591, 1.34759827591}, "expected_revision": "18446744073709551615", "expected_rotation_revision": "18446744073709551615", "expected_scale_revision": "18446744073709551615"},
			"ray":     map[string]any{"target": target, "holding": true, "origin": []float64{991.437939265237, -751.228463756237, 926.243648563275}, "point": point},
		},
	}
	data, _ := json.Marshal(packet)
	encoded, err := encodeLatest(data)
	if err != nil {
		t.Fatal(err)
	}
	wire, err := protocol.Encode(protocol.Message{"type": "latest", "key": "kokoverse.pose.v2", "seq": uint64(100), "value": encoded})
	if err != nil {
		t.Fatal(err)
	}
	if len(wire) > protocol.MaxRealtimeFrame-128 {
		t.Fatalf("latest size %d exceeds relay budget", len(wire))
	}
	decoded, err := decodeLatest(encoded)
	if err != nil || !bytes.Equal(data, decoded) {
		t.Fatalf("round trip failed: %v", err)
	}
	legacy, err := decodeLatest(string(data))
	if err != nil || !bytes.Equal(data, legacy) {
		t.Fatal("legacy decode failed")
	}
}

func TestOfferLatestCoalescesAndNeverBlocks(t *testing.T) {
	latest := make(chan outbound, 1)
	ctx := context.Background()
	if !offerLatest(ctx, latest, outbound{Sequence: "1"}) {
		t.Fatal("first offer rejected")
	}
	// No consumer: a newer pose replaces the stale one instead of blocking.
	done := make(chan bool, 1)
	go func() { done <- offerLatest(ctx, latest, outbound{Sequence: "2"}) }()
	select {
	case ok := <-done:
		if !ok {
			t.Fatal("second offer rejected")
		}
	case <-time.After(5 * time.Second):
		t.Fatal("offerLatest blocked without a consumer")
	}
	if got := <-latest; got.Sequence != "2" {
		t.Fatalf("expected newest pose to win, got sequence %q", got.Sequence)
	}
	// A cancelled context rejects the offer immediately.
	cancelled, cancel := context.WithCancel(context.Background())
	cancel()
	if offerLatest(cancelled, latest, outbound{Sequence: "3"}) {
		t.Fatal("offer accepted after cancellation")
	}
}

func TestNextRetryDelayBacksOffAndResets(t *testing.T) {
	delay := retryMinDelay
	for _, want := range []time.Duration{4 * time.Second, 8 * time.Second, 16 * time.Second, 32 * time.Second, retryMaxDelay, retryMaxDelay} {
		delay = nextRetryDelay(delay, time.Second)
		if delay != want {
			t.Fatalf("delay = %v, want %v", delay, want)
		}
	}
	if got := nextRetryDelay(retryMaxDelay, 2*time.Minute); got != retryMinDelay {
		t.Fatalf("healthy run should reset delay, got %v", got)
	}
}

func TestMalformedOrOversizedLatestRejected(t *testing.T) {
	for _, s := range []string{"z1:!", "z1:AAAA"} {
		if _, err := decodeLatest(s); err == nil {
			t.Fatal("accepted invalid compression")
		}
	}
	encoded, _ := encodeLatest(bytes.Repeat([]byte{'x'}, maxFrame+1))
	if _, err := decodeLatest(encoded); err == nil {
		t.Fatal("accepted oversized frame")
	}
}
