@0xceb71d1333e035f1;

struct Message {
    union {
        register :group {
            name @0 :Text;
            pubkey @12 :Data;
        }

        send :group {
            dest @1 :Text;
            body @2 :Data;
            union {
                nonce @13 :Data;
                unencrypted @14 :Void;
            }
        }


        join :group {
            channel @6 :Text;
        }

        part :group {
            channel @7 :Text;
        }


        whois :group {
            name @9 :Text;
        }

        theyare :group {
            name @10 :Text;
            pubkey @11 :Data;
        }


        response :group {
            body @8 :Text;
        }

        relay :group {
            source @3 :Text;
            dest @4 :Text;
            body @5 :Data;
            union {
                nonce @15 :Data;
                unencrypted @16 :Void;
            }
        }
    }
}
