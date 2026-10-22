package main

// Keep the pinned SDK's fragmentation, receipts and ordering unchanged while
// tunneling its local QUIC transport through the relay's public TLS WebSocket.
import (
	"context"
	"crypto/ecdsa"
	"crypto/elliptic"
	"crypto/rand"
	"crypto/tls"
	"crypto/x509"
	"crypto/x509/pkix"
	"encoding/hex"
	"errors"
	"math/big"
	"net"
	"net/http"
	"net/url"
	"os"
	"strings"
	"sync"
	"time"

	"github.com/coder/websocket"
	"github.com/katteni-kaihatu/kokosync/protocol"
	kokosync "github.com/katteni-kaihatu/kokosync/sdk/go"
	"github.com/quic-go/quic-go"
)

type hostedBridge struct {
	listener *quic.Listener
	ws       *websocket.Conn
	cancel   context.CancelFunc
	once     sync.Once
	writeMu  sync.Mutex
}

func (b *hostedBridge) close() {
	b.once.Do(func() {
		b.cancel()
		_ = b.ws.CloseNow()
		_ = b.listener.Close()
	})
}

func (b *hostedBridge) write(ctx context.Context, frame []byte) error {
	b.writeMu.Lock()
	defer b.writeMu.Unlock()
	writeCtx, cancel := context.WithTimeout(ctx, 5*time.Second)
	defer cancel()
	return b.ws.Write(writeCtx, websocket.MessageBinary, frame)
}

func loopbackCertificate() (tls.Certificate, error) {
	key, err := ecdsa.GenerateKey(elliptic.P256(), rand.Reader)
	if err != nil {
		return tls.Certificate{}, err
	}
	serial, err := rand.Int(rand.Reader, new(big.Int).Lsh(big.NewInt(1), 128))
	if err != nil {
		return tls.Certificate{}, err
	}
	template := &x509.Certificate{SerialNumber: serial, Subject: pkix.Name{CommonName: "Kokoverse local WebSocket bridge"}, NotBefore: time.Now().Add(-time.Minute), NotAfter: time.Now().Add(24 * time.Hour), KeyUsage: x509.KeyUsageDigitalSignature, ExtKeyUsage: []x509.ExtKeyUsage{x509.ExtKeyUsageServerAuth}, IPAddresses: []net.IP{net.ParseIP("127.0.0.1")}}
	der, err := x509.CreateCertificate(rand.Reader, template, template, &key.PublicKey, key)
	if err != nil {
		return tls.Certificate{}, err
	}
	return tls.Certificate{Certificate: [][]byte{der}, PrivateKey: key}, nil
}

func openHostedBridge(parent context.Context, address string, header http.Header) (*hostedBridge, error) {
	u, err := url.Parse(address)
	if err != nil || u.User != nil || u.Host == "" || (u.Scheme != "wss" && u.Scheme != "ws") {
		return nil, errors.New("invalid WebSocket relay URL")
	}
	if u.Scheme == "ws" {
		ip := net.ParseIP(u.Hostname())
		if ip == nil || !ip.IsLoopback() {
			return nil, errors.New("remote relay connections require wss://")
		}
	}
	dialCtx, stop := context.WithTimeout(parent, 10*time.Second)
	ws, response, err := websocket.Dial(dialCtx, address, &websocket.DialOptions{HTTPHeader: header})
	stop()
	if err != nil {
		if response != nil && response.StatusCode == http.StatusUnauthorized {
			return nil, errors.New("relay login rejected; check username and password")
		}
		return nil, err
	}
	ws.SetReadLimit(protocol.MaxFrame)
	certificate, err := loopbackCertificate()
	if err != nil {
		_ = ws.CloseNow()
		return nil, err
	}
	listener, err := quic.ListenAddr("127.0.0.1:0", &tls.Config{MinVersion: tls.VersionTLS13, Certificates: []tls.Certificate{certificate}, NextProtos: []string{"kokosync/1"}}, &quic.Config{EnableDatagrams: true, KeepAlivePeriod: 10 * time.Second})
	if err != nil {
		_ = ws.CloseNow()
		return nil, err
	}
	ctx, cancel := context.WithCancel(parent)
	b := &hostedBridge{listener: listener, ws: ws, cancel: cancel}
	go b.serve(ctx)
	return b, nil
}

func (b *hostedBridge) serve(ctx context.Context) {
	defer b.close()
	conn, err := b.listener.Accept(ctx)
	if err != nil {
		return
	}
	stop := context.AfterFunc(ctx, func() { _ = conn.CloseWithError(0, "bridge closed") })
	defer stop()
	defer conn.CloseWithError(0, "bridge closed")
	stream, err := conn.AcceptStream(ctx)
	if err != nil {
		return
	}
	go func() {
		defer b.close()
		for {
			frame, err := protocol.ReadFrame(stream)
			if err != nil || b.write(ctx, frame) != nil {
				return
			}
		}
	}()
	go func() {
		defer b.close()
		for {
			frame, err := conn.ReceiveDatagram(ctx)
			if err != nil || b.write(ctx, frame) != nil {
				return
			}
		}
	}()
	for {
		kind, frame, err := b.ws.Read(ctx)
		if err != nil || kind != websocket.MessageBinary {
			return
		}
		message, err := protocol.Decode(frame)
		if err != nil {
			return
		}
		if message["type"] == "latest" || message["type"] == "live" {
			if len(frame) <= protocol.MaxRealtimeFrame {
				_ = conn.SendDatagram(frame)
			}
		} else {
			_ = stream.SetWriteDeadline(time.Now().Add(5 * time.Second))
			if protocol.WriteFrame(stream, frame) != nil {
				return
			}
		}
	}
}

func connectRelay(ctx context.Context, address, session, peer string, insecure bool) (*kokosync.Client, func(), error) {
	cleanup := func() {}
	if strings.HasPrefix(address, "wss://") || strings.HasPrefix(address, "ws://") {
		header := make(http.Header)
		if username := os.Getenv("KOKOSYNC_USERNAME"); username != "" {
			request := &http.Request{Header: header}
			request.SetBasicAuth(username, os.Getenv("KOKOSYNC_PASSWORD"))
		}
		bridge, err := openHostedBridge(ctx, address, header)
		if err != nil {
			return nil, cleanup, err
		}
		address, insecure = bridge.listener.Addr().String(), true
		cleanup = bridge.close
	}
	joinCtx, cancel := context.WithTimeout(ctx, 10*time.Second)
	defer cancel()
	client, err := kokosync.Connect(joinCtx, kokosync.Options{Address: address, Room: "kokoverse/" + session, Peer: peer, InsecureDevelopment: insecure})
	if err != nil {
		cleanup()
	}
	return client, cleanup, err
}

// The server atomically admits only one "host" peer. Keep the chosen role for
// reconnects so a transient disconnect does not replace an existing world.
func connectAutomatic(ctx context.Context, address, session string, peer *string, insecure bool) (*kokosync.Client, func(), error) {
	if *peer == "auto" {
		client, cleanup, err := connectRelay(ctx, address, session, "host", insecure)
		if err == nil {
			*peer = "host"
			return client, cleanup, nil
		}
		var rejection *kokosync.Error
		if !errors.As(err, &rejection) || rejection.Code != "Conflict" {
			return nil, cleanup, err
		}
		var token [16]byte
		if _, err := rand.Read(token[:]); err != nil {
			return nil, func() {}, err
		}
		*peer = "guest-" + hex.EncodeToString(token[:])
	}
	return connectRelay(ctx, address, session, *peer, insecure)
}
