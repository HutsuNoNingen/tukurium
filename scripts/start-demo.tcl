#!/usr/bin/env tclsh
# vim: set sts=2 sw=2 et :

##############################
# Script configuration
##############################

array set ::option_types { -Headless switch -Session arg -WorldFile arg }
array set ::options { -Headless 0 -WorldFile "" }

# These are the executable program files we need.
array set ::exe_paths {
  adapter ./.tools/kokosync-adapter
  engine ./target/debug/kokoverse-engine
  relay ./.tools/kokosync-relay
}

# These are the processes to run.
set ::service_names [list relay host host-adapter guest guest-adapter]
set ::service_names_not_headless [list host-gui guest-gui]
array set ::service_specs {
  relay {list $::exe_paths(relay) --http "127.0.0.1:48000" --quic "127.0.0.1:48001"}
  host {
    set result [list $::exe_paths(engine) --session $::options(-Session) --port 47000 --transport-port 47001]
    if {$::options(-WorldFile) ne {}} { lappend result --load $::options(-WorldFile) }
    list {*}$result
  }
  guest {list $::exe_paths(engine) --role guest --session $::options(-Session) --port 47010 --transport-port 47011}
  host-adapter {list $::exe_paths(adapter) -session $::options(-Session) --engine 127.0.0.1:47001 --peer host --insecure-development}
  guest-adapter {list $::exe_paths(adapter) -session $::options(-Session) --engine 127.0.0.1:47011 --peer guest --insecure-development}
  host-gui {list $::exe_paths(godot) --path client-godot -- --port=47000 --session=$::options(-Session)}
  guest-gui {list $::exe_paths(godot) --path client-godot -- --port=47010 --session=$::options(-Session)}
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

  # If this isn't a headless server, add some more things
  if {!$::options(-Headless)} {
    lappend ::service_names {*}$::service_names_not_headless

    # TODO: Windows-compatible version of this?
    set ::exe_paths(godot) [exec which godot]
  }

  # Ensure everything exists
  foreach key [array names ::exe_paths] {
    set path [EXE $::exe_paths($key)]
    if {![file exists $path]} {
      error [L err-exe-not-found $path]
    }
    puts [L found-executable $key $path]
  }

  # Process arguments for services
  foreach service_name $::service_names {
    set ::service_args($service_name) [eval $::service_specs($service_name)]
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
  tk appname "kokoverse-demo-supervisor"
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
