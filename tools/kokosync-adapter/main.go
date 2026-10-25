// The adapter owns delivery only; the Rust engine owns all world semantics.
package main

import (
	"bytes"
	"compress/zlib"
	"context"
	"encoding/base64"
	"encoding/binary"
	"encoding/json"
	"errors"
	"flag"
	"fmt"
	"io"
	"net"
	"os"
	"strconv"
	"strings"
	"sync"
	"time"
)

const maxFrame = 1024 * 1024

// Keep combined pose, full transform preview and laser below the datagram budget.
// Reliable messages retain the JSON format; older uncompressed poses can be read.
func encodeLatest(data []byte) (string, error) {
	var buffer bytes.Buffer
	w := zlib.NewWriter(&buffer)
	if _, err := w.Write(data); err != nil {
		return "", err
	}
	if err := w.Close(); err != nil {
		return "", err
	}
	return "z1:" + base64.StdEncoding.EncodeToString(buffer.Bytes()), nil
}

func decodeLatest(text string) ([]byte, error) {
	if !strings.HasPrefix(text, "z1:") {
		return []byte(text), nil
	}
	data, err := base64.StdEncoding.DecodeString(text[3:])
	if err != nil {
		return nil, err
	}
	r, err := zlib.NewReader(bytes.NewReader(data))
	if err != nil {
		return nil, err
	}
	defer r.Close()
	data, err = io.ReadAll(io.LimitReader(r, maxFrame+1))
	if len(data) > maxFrame {
		return nil, errors.New("latest frame too large")
	}
	return data, err
}

func run(engine, address, session string, peer *string, insecure bool) error {
	conn, err := net.DialTimeout("tcp", engine, 5*time.Second)
	if err != nil {
		return err
	}
	defer conn.Close()
	var mu sync.Mutex
	send := func(value any) error {
		data, err := json.Marshal(value)
		if err != nil {
			return err
		}
		if len(data) > maxFrame {
			return errors.New("frame too large")
		}
		mu.Lock()
		defer mu.Unlock()
		conn.SetWriteDeadline(time.Now().Add(5 * time.Second))
		if err = binary.Write(conn, binary.BigEndian, uint32(len(data))); err != nil {
			return err
		}
		_, err = io.Copy(conn, &byteReader{data: data})
		return err
	}
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	client, closeRelay, err := connectAutomatic(ctx, address, session, peer, insecure)
	if err != nil {
		send(map[string]any{"type": "error", "message": err.Error()})
		return err
	}
	defer client.Close()
	defer closeRelay()
	if err = send(map[string]any{"type": "ready", "self_peer": *peer, "peers": client.Info()["peers"]}); err != nil {
		return err
	}
	// A dedicated reader keeps SDK receive queues flowing while a send waits for a receipt.
	failures := make(chan error, 4)
	go func() {
		for {
			select {
			case <-client.Done():
				failures <- errors.New("KokoSync disconnected")
				return
			case <-ctx.Done():
				return
			case m := <-client.Messages():
				kind, _ := m["type"].(string)
				var event map[string]any
				if kind == "message" || kind == "peer_message" || kind == "latest" {
					if (kind == "latest" && m["key"] != "kokoverse.pose.v2") || (kind != "latest" && m["topic"] != "kokoverse.v2") {
						continue
					}
					// JSON is carried as a string so MessagePack maps never change its shape.
					text, ok := m["value"].(string)
					if !ok {
						continue
					}
					var body any
					data := []byte(text)
					if kind == "latest" {
						var err error
						data, err = decodeLatest(text)
						if err != nil {
							continue
						}
					}
					if json.Unmarshal(data, &body) != nil {
						continue
					}
					eventType := "message"
					if kind == "latest" {
						eventType = "pose"
					}
					event = map[string]any{"type": eventType, "peer": m["peer"], "publisher_epoch": m["publisherEpoch"], "body": body}
				} else if kind == "peer_joined" || kind == "peer_left" {
					event = map[string]any{"type": kind, "peer": m["peer"], "publisher_epoch": m["publisherEpoch"]}
				} else {
					continue
				}
				if err := send(event); err != nil {
					failures <- err
					return
				}
			}
		}
	}()
	type outbound struct {
		Type     string          `json:"type"`
		To       *string         `json:"to"`
		Body     json.RawMessage `json:"body"`
		Sequence string          `json:"sequence"`
	}
	reliable := make(chan outbound, 64)
	latest := make(chan outbound, 1)
	// Loss-tolerant poses never wait behind reliable SendTo acknowledgement.
	go func() {
		for {
			select {
			case <-ctx.Done():
				return
			case command := <-latest:
				sequence, parseErr := strconv.ParseUint(command.Sequence, 10, 64)
				if parseErr != nil {
					failures <- parseErr
					return
				}
				encoded, err := encodeLatest(command.Body)
				if err != nil {
					failures <- err
					return
				}
				if err := client.Latest(ctx, "kokoverse.pose.v2", sequence, encoded); err != nil {
					failures <- err
					return
				}
			}
		}
	}()
	go func() {
		for {
			var command outbound
			select {
			case <-ctx.Done():
				return
			case command = <-reliable:
			}
			sendCtx, stop := context.WithTimeout(ctx, 5*time.Second)
			var err error
			if command.To != nil {
				_, err = client.SendTo(sendCtx, *command.To, "kokoverse.v2", string(command.Body))
			} else {
				_, err = client.Publish(sendCtx, "kokoverse.v2", string(command.Body))
			}
			stop()
			if err != nil {
				send(map[string]any{"type": "error", "message": err.Error()})
				failures <- err
				return
			}
		}
	}()
	go func() {
		for {
			var size uint32
			if err := binary.Read(conn, binary.BigEndian, &size); err != nil {
				failures <- err
				return
			}
			if size == 0 || size > maxFrame {
				failures <- errors.New("invalid frame size")
				return
			}
			data := make([]byte, size)
			if _, err := io.ReadFull(conn, data); err != nil {
				failures <- err
				return
			}
			var command outbound
			if err := json.Unmarshal(data, &command); err != nil {
				failures <- err
				return
			}
			switch command.Type {
			case "send":
				select {
				case reliable <- command:
				case <-ctx.Done():
					return
				default:
					failures <- errors.New("reliable queue full")
					return
				}
			case "latest":
				select {
				case <-latest:
				default:
				}
				latest <- command
			}
		}
	}()
	return <-failures
}

type byteReader struct{ data []byte }

func (r *byteReader) Read(p []byte) (int, error) {
	if len(r.data) == 0 {
		return 0, io.EOF
	}
	n := copy(p, r.data)
	r.data = r.data[n:]
	return n, nil
}
func main() {
	engine := flag.String("engine", "127.0.0.1:47001", "local Rust engine")
	address := flag.String("address", "127.0.0.1:48001", "KokoSync QUIC address or wss:// relay URL")
	session := flag.String("session", "demo", "tukurium session")
	peer := flag.String("peer", "host", "unique participant name")
	insecure := flag.Bool("insecure-development", false, "allow localhost development certificate")
	flag.Parse()
	for {
		if err := run(*engine, *address, *session, peer, *insecure); err != nil {
			fmt.Fprintln(os.Stderr, "Disconnected; retrying in 2s:", err)
		}
		time.Sleep(2 * time.Second)
	}
}
