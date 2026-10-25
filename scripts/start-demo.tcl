#!/usr/bin/env tclsh
# vim: set sts=2 sw=2 et :

##############################
# Script configuration
##############################

array set ::option_types {
  -Headless switch
  -Local switch -Online switch
  -Session arg -WorldFile arg
  -Profile arg
}
array set ::options {
  -Headless 0
  -Local 0 -Online 0
  -WorldFile ""
  -Profile "default"
}

# These are the executable program files we need.
array set ::exe_paths {
  adapter {./.tools/kokosync-adapter}
  asset-server {./target/debug/kokoverse-asset-server}
  engine {./target/debug/kokoverse-engine}
  relay {./.tools/kokosync-relay}
}
array set ::exe_path_generators {
  godot {exec which godot}
}

# These are the processes to run.
array set ::service_modes {
  demo {relay host-engine host-adapter host-gui guest-engine guest-adapter guest-gui}
  demo-headless {relay host-engine host-adapter guest-engine guest-adapter}
  local {relay local-asset-server local-engine local-gui}
  local-headless {relay local-asset-server local-engine local-adapter}
  online {relay online-asset-server client-engine client-adapter client-gui}
}
set ::service_names [list]

array set ::service_specs {
  relay {list relay --http "127.0.0.1:48000" --quic "127.0.0.1:48001"}
  local-asset-server {list asset-server --listen 127.0.0.1:8787 --data-dir .run/local/server --relay 127.0.0.1:48001}

  host-engine {
    set result [list engine --session $::options(-Session) --port 47000 --transport-port 47001]
    if {$::options(-WorldFile) ne {}} { lappend result --load $::options(-WorldFile) }
    list {*}$result
  }
  host-adapter {list adapter -session $::options(-Session) --engine 127.0.0.1:47001 --peer host --insecure-development}
  host-gui {list godot --path client-godot -- --port=47000 --session=$::options(-Session)}

  guest-engine {list engine --role guest --session $::options(-Session) --port 47010 --transport-port 47011}
  guest-adapter {list adapter -session $::options(-Session) --engine 127.0.0.1:47011 --peer guest --insecure-development}
  guest-gui {list godot --path client-godot -- --port=47010 --session=$::options(-Session)}

  local-engine {list engine --server http://127.0.0.1:8787 --port 47000 --transport-port 47001 --profile .run/Kokoverse/profiles/$::options(-Profile)/profile.dat --data-dir .run/local/$::options(-Profile) --asset-dir .run/local/$::options(-Profile)/assets --adapter [EXE $::exe_paths(adapter)] --session $::options(-Session) --console --console-title "tukurium Engine ($::options(-Profile))"}
  local-gui {list godot --path client-godot -- --port=47000 --session=lobby --close-with-engine --lof-file}
}
array set ::service_args {}

array set ::service_process_handles {}

##############################
# Locales
##############################

array set ::locale {}

proc set_locale_en {} {
  array set ::locale {
    err-session-id-invalid "Session must be 1-80 letters, numbers, underscores or hyphens."
    err-need-args-1 "\"%0\" takes one argument"
    err-exe-not-found "\"%0\" not found. Did you build the project first? Did you make sure that you are running this script from the project root?"
    err-service-not-found "Service \"%0\" not found. Please specify it in the ::exe_paths array, or write code for the ::exe_path_generators array if the path is to be computed at runtime."

    found-executable "Found executable \"%0\" at \"%1\""
  }
}

proc set_locale_ja {} {
  array set ::locale {
    found-executable "「%0」っていうEXEは「%1」に見つけた"
  }
}

proc set_locale_ko {} {
  array set ::locale {
  }
}

proc L args {
  set extra_args [lassign $args msg_key]
  set msg $::locale($msg_key)
  for {set i 0} {$i < [llength $extra_args]} {incr i} {
    set msg [regsub -all -- "%${i}" $msg [lindex $extra_args $i]]
  }
  return $msg
}

##############################
# Entry point
##############################

proc main args {
  # Load locale first
  init_locale

  # Parse arguments
  read_args {*}$args

  # If needed, generate a session ID
  if {![info exists ::options(-Session)]} { set ::options(-Session) [generate_session_id] }
  # Validate session ID
  if {![regexp -- {^[A-Za-z0-9_-]{1,80}$} $::options(-Session)]} { error [L err-session-id-invalid] }

  # Select a mode
  if {$::options(-Local)} {
    set mode local
    if {$::options(-Headless)} { set mode local-headless }
  } elseif {$::options(-Online)} {
    # TODO!
    set mode online
  } else {
    set mode demo
    if {$::options(-Headless)} { set mode demo-headless }
  }

  set ::service_names $::service_modes($mode)

  # Process arguments for services
  # Also ensure everything exists
  foreach service_name $::service_names {
    set prog_args [lassign [eval $::service_specs($service_name)] prog_cmd]
    if {![info exists ::exe_paths($prog_cmd)]} {
      if {![info exists ::exe_path_generators($prog_cmd)]} {
        error [L err-service-not-found $prog_cmd]
      }
      set ::exe_paths($prog_cmd) [eval $::exe_path_generators($prog_cmd)]
    }
    set path [EXE $::exe_paths($prog_cmd)]
    if {![file exists $path]} {
      error [L err-exe-not-found $path]
    }
    puts [L found-executable $prog_cmd $path]
    set ::service_args($service_name) [list $::exe_paths($prog_cmd) {*}$prog_args]
    puts "Arguments for service \"$service_name\": $::service_args($service_name)"
  }

  # Print some information
  puts "Session ID: \"$::options(-Session)\""

  # Everything is OK! (hopefully.) Start Tk GUI.
  init_tk

  # Start process supervisors
  foreach service_name $::service_names {
    after 0 [list start_service $service_name]
  }

  # Run event loop!
  set ::stop_event_loop {}
  vwait ::stop_event_loop

  # Close main window if it still exists
  catch {destroy .} ;# Suppress error

  # Clean up services
  foreach service_name $::service_names {
    stop_service $service_name
  }
}

proc init_locale {} {
  set_locale_en
  set lang $::env(LANG)
  if {[regexp -- {^ja} $lang]} {
    set_locale_ja
  } elseif {[regexp -- {^ko} $lang]} {
    set_locale_ko
  }
}

proc init_tk {} {
  package require Tk 8.6
  tk appname "tukurium-demo-supervisor"
  puts "appname \"[tk appname]\""
  # Make sure the event loop stops when we're done
  bind . <Destroy> { set ::stop_event_loop {ok} }

  # Build tabbed "notebook" widget
  grid [ttk::notebook .tabs] -sticky nswe
  foreach service_name $::service_names {
    set svc .tabs.$service_name

    # Build standard view for each service
    .tabs add [ttk::frame $svc] -text $service_name
    grid [text $svc.text] [ttk::scrollbar $svc.text_y_scroll -orient vertical] -sticky nswe

    # Set up scrollbars
    $svc.text_y_scroll configure -command [list $svc.text yview]
    $svc.text configure -yscrollcommand [list $svc.text_y_scroll set]

    # Expand text box
    grid columnconfigure $svc $svc.text -weight 1
    grid rowconfigure $svc $svc.text -weight 1
  }
  # Expand "notebook"
  grid columnconfigure . .tabs -weight 1
  grid rowconfigure . .tabs -weight 1
}

##############################
# Process supervisor
##############################

proc start_service {service_name} {
  if {[catch {
    # Open process
    puts "Starting service \"$service_name\""
    set ::service_process_handles($service_name) [open "|$::service_args($service_name) 2>@1" "r+b"]

    # Configure channel
    set process $::service_process_handles($service_name)
    chan configure $process -blocking false -buffering none
    fileevent $process readable [list on_service_readable $service_name]

    puts "Service \"$service_name\" started."

  } result]} {
    # Fail and stop event loop
    set ::stop_event_loop [list closed $service_name]
    puts "ERROR: Process supervisor for \"$service_name\" stopped unexpectedly: $result"
  }
}

proc stop_service {service_name} {
  if {[info exists ::service_process_handles($service_name)]} {
    puts "Stopping service \"$service_name\""
    set process $::service_process_handles($service_name)

    # On non-Windows systems, kill the process
    catch {exec kill [pid $process]}

    # Close the file descriptor
    catch {close $process}

    # Clean up
    catch {unset ::service_process_handles($service_name)}

    puts "Service \"$service_name\" stopped."
  }
}

proc on_service_readable {service_name} {
  if {[catch {
    set process $::service_process_handles($service_name)
    set data [read $process]
    if {$data eq {}} {
      # Fail and stop event loop
      # TODO: Consider respawning! --GM
      set ::stop_event_loop [list closed $service_name]
      puts "ERROR: Unexpected EOF for service \"$service_name\""
      fileevent $process readable {}
    } else {
      set svc .tabs.$service_name
      $svc.text insert end $data
    }
  } result]} {
    # Fail and stop event loop
    set ::stop_event_loop [list closed $service_name]
    puts "ERROR: Process supervisor for \"$service_name\" stopped unexpectedly on read: $result"
  }
}

##############################
# Helpers
##############################

proc EXE {path} {
  switch -- $::tcl_platform(platform) {
    windows { return "${path}.exe" }
    default { return "${path}" }
  }
}

proc read_args args {
  set i 0
  while {$i < [llength $args]} {
    set key [lindex $args $i]; incr i
    switch -- $::option_types($key) {
      switch {
        set ::options($key) 1
      }
      arg {
        if {$i >= [llength $args]} { error [L err-need-args-1 $key] }
        set ::options($key) [lindex $args $i]; incr i
      }
      default { error "BUG! FIX ME!" }
    }
  }
}

proc generate_session_id {} {
  # v4 UUID (or "GUID" in Microsoftese)
  # ????????-????-4???-!???-????????????
  # ! in [89ab]
  # ? in [0-9a-f]
  set variant_nybble [format "%x" [expr {max(8, min(11, 8+int(floor(rand()*4))))}]]
  set guid "[random_hex 8]-[random_hex 4]-4[random_hex 3]-${variant_nybble}[random_hex 3]-[random_hex 12]"
  return "demo-$guid"
}

proc random_hex {count} {
  set result [list]
  for {set i 0} {$i < $count} {incr i} {
    lappend result [format "%x" [expr {max(0, min(15, int(floor(rand()*16))))}]]
  }
  return [join $result ""]
}

##############################
# Let's go!
##############################

main {*}$argv
