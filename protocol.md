# Description

## Networking

Essentially two protocols, server-server and client-server protocols.  Servers
form an undirected graph, clients may connect to any node.  The topology of
this graph is left unspecified (save that it must be connected), and the
protocol should support any permutation.

Note: Can ``inner servers'', i.e, servers that only implement the server-server
and not client-server protocol, exist? Likely yes, logging in will occur similarly to irc and connection attempts will select only from servers that offer the client protocol, and relay-only servers will implement the routing table of other servers.

Note: How are servers identified (likely a server-username)? Possibly by their hostname?

## Crypto

Client-server and server-server communications are all encrypted, and all
servers are authenticated.  (Do clients themselves need to be authenticated?)

Note: How much to we have to think about, e.g., OTR here?

There is a second level of crypto at the message level, in order to decrease
the level of information that needs to be trusted with the server.  Ideally
we prevent the server from knowing anything that it doesn't need to to
correctly relay messages and manage channel/user state.

This means that the client-server protocol actually has two layers: the part of
the protocol that the server needs to care about, and the part that only
clients need to care about.

## Abstractions and State

The protocol concerns two major entities: users (individuals) and channels
(dynamic lists of individuals).  Wherever possible these two concepts are
unified.

Every server needs to have a global list of users/channels, along with which
server(s) to talk to in order to send messages to that user/channel.  (This
is essentially a routing table).  Each server also needs to know whether or 
not a given user is online.

Users are also paired with their identifying public key in this list.  Every
user obligatorily has a unique public key associated with them, and every
server must know about this key. Channels likewise will have some cryptographic keys which may not be known to the server (if this is possible).

Individual users and channels live on a specific server (for users, this is the
server they are directly connected to; for channels, this is generally the
server of the user who created it).

The channel state on a server is similar to the global state: it contains both
a permanent list of user/key pairs and ephemeral list of present users.
Depending on the kind of channel, this permanent list may be a whitelist of
allowed users (invite-only channels), or a blacklist of banned users (public
channels).

There may be a mechanism for taking some amount of channel off the server, or
allowing channel ops/owners to interact with the protocol directly as the
channel.  TODO: elaborate on what this entails.

Both user and channel state also contain a list of modes, which may have
arguments (more on this in the modes section), and a realname (users) or
topic (channels).

# Protocol Details

## User/Channel State

Users have 3 fields associated with them: a username, realname, and nickname.
The username has the most stringent requirements, with a minimum length of 3
characters, maximum length of 32, and may only contain printable ascii
characters.

Realnames have no restrictions other than a maximum length (TBD) and that they
may not contain a null byte.

Nicknames have a subtler set of requirements (elaborate), and exist per channel
(that is, a single user can have different nicks in different channels).
Nicknames cannot be reserved, except that no user in any channel may have a
nickname that is string-equivalent to a different user's username.

Note: What of these are case-sensitive?

Note: Potentially realnames and nicknames can be user and channel modes,
respectively.

Channels have only one name associated with them, and it must begin with a
hash (`#') character.

Note: Potentially stable channel modes (i.e.: ones that cannot change after
the channel exists) can be marked with different initial characters than hash.

## Modes

Note: At some point we should probably make a point of cleaning up the fact
that some of these (away) are present or not, others (invite-only/public)
partition a possibility space, and others (op, voice) may exist multiple times
with different arguments.

Note: Some of these may be automatically set by the server (esp. "user=nick"

### User

* Away: marks the user as away, may take an optional message argument.
* Oper: marks a network oper

### Channel

* Invite-Only v. Public
* Listed v. Unlisted
* Anonymous: nicknames and usernames of source messages aren't reported
* Anonymous': usernames of source messages aren't reported (nicks are)
* Nick-free: cannot have nicks
* Democratic/Anarchic/Irc-plays-irc: alternative styles of moderation
* Unencrypted
* Persistant: maintains state after all users leave
* Quiet: Unvoiced users can't talk
* Loud: Unhearing users can't listen

(the following modes all take a single username argument, in addition to their
others)

* Ban/Invite: user is not/is allowed in channel
* Op/Hop: user has special privileges to modify/query channel state (elaborate)

* Mute: cannot speak
* Voice: can speak in quiet channels
* Deaf: cannot hear
* Hearing: can hear in loud channels

* User=nick: user's nick is string-equivilent to their username

# Message Format
(Possibly should have a name other than "Message"?)

Messages are structs with a command field and an argument_block field.  The
command field is always exposed to the server (i.e.: unencrypted), and
determines the format of the argument_block field, which is another struct
which may or may not be server-inspectable.

## Commands

Each command is listed, followed by a description of the argument structure
associated with it and what it does.

* Talk: user/channel (unencrypted), source (unencrypted),
        flag (encrypted), content (encrypted)

    Server relays the entire message to the the user/channel (target) specified.

    The flag argument specifies things like what in IRC or CTCP ACTION or
    NOTICE commands.  (TODO: elaborate on possible values here).

    The content is the actual message content, which the client is expected to
    render/respond to at their own discretion (using the flag as a hint,
    basically).  This field may contain formatting as specified in the
    `formatting` section.

* Join: user/channel (unencrypted), source (encrypted?),
        nickname (encrypted?)

    Joining a user is likely relevant for OTR crypto handshakes.

* Part: user/channel (unencrypted), source (encrypted?),
        message (encrypted)

    Parting a user is likely relevant for OTR crypto handshakes.

* Register: user (unencrypted), public key (unencrypted)
    
    Persistantly associates a username with a public key (in all servers).

* Connect: user (unencrypted)
    
    Authenticates the connection as a specific user.  This may need to be more
    complicated than a single message, depending on the crypto used.

* Unregister: user (unencrypted)

    Removes a user/publickey pair from the global state.  Like connect, may
    require more complicated authentication.

* Quit: message (encrypted?), source (encrypted?)

    Disconnects from the server, notifying all channels the user is in (and all
    other users the user is talking to?).

* Kick: channel (unencrypted), user (encrypted),
        source (encrypted?), message (encrypted)
    
    Forcibly removes a user from a channel.  May require certain permissions.

* Invite: channel (unencrypted), user (encrypted?), source (encrypted?)

    Allows a user to join a channel.  May send a singal or key to that user.

* Nick: channel (unencrypted), source (encrypted?), nickname (encrypted)
    
    (Tries to) set nick in channel to new nickname.

* Whois: user/channel (unencrypted)

    Returns information on the user

* Mode: user/channel (unencrypted), modestring (encrypted), source (encrypted?)

    If the modestring is null, queries the user or channel's modes, otherwise
    tries to set them.  (TODO: more details on what the modestring can be)

* List: channel (unencrypted)
    
    If the argument is null, returns a list of channels on the server,
    otherwise returns a list of users on the channel given.

* Topic: user/channel (unencrypted), topic (encrypted)

    If the topic is null, request the realname/topic of the specified user/channel,
    otherwise try to set it.

## Formatting
    
Bold, italic, underline and reverse video are toggled with specific low
ascii values.

Colors can be specified in 8, 16, or 256-value, or 3-byte mode.

Clients are not required to respect formatting, messages should degrade
nicely.
