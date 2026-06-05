#import <Foundation/Foundation.h>
#import <IOBluetooth/IOBluetooth.h>
#include <stdint.h>
#include <string.h>

@interface BoseRFCOMMDelegate : NSObject <IOBluetoothRFCOMMChannelDelegate>
@property(nonatomic, strong) NSMutableData *data;
@property(nonatomic) BOOL closed;
@end

@implementation BoseRFCOMMDelegate
- (instancetype)init {
    self = [super init];
    if (self) {
        _data = [NSMutableData data];
        _closed = NO;
    }
    return self;
}

- (void)rfcommChannelData:(IOBluetoothRFCOMMChannel *)rfcommChannel
                     data:(void *)dataPointer
                   length:(size_t)dataLength {
    (void)rfcommChannel;
    if (dataPointer != NULL && dataLength > 0) {
        [self.data appendBytes:dataPointer length:dataLength];
    }
}

- (void)rfcommChannelClosed:(IOBluetoothRFCOMMChannel *)rfcommChannel {
    (void)rfcommChannel;
    self.closed = YES;
}
@end

static void bose_set_error(char *error, uint16_t error_cap, NSString *message) {
    if (error == NULL || error_cap == 0) {
        return;
    }
    const char *utf8 = [message UTF8String];
    if (utf8 == NULL) {
        utf8 = "unknown error";
    }
    size_t len = strlen(utf8);
    if (len >= error_cap) {
        len = error_cap - 1;
    }
    memcpy(error, utf8, len);
    error[len] = '\0';
}

static IOBluetoothDevice *bose_device_for_address(NSString *address) {
    IOBluetoothDevice *device = [IOBluetoothDevice deviceWithAddressString:address];
    if (device != nil) {
        return device;
    }

    NSString *hyphenated = [address stringByReplacingOccurrencesOfString:@":" withString:@"-"];
    device = [IOBluetoothDevice deviceWithAddressString:hyphenated];
    if (device != nil) {
        return device;
    }

    NSString *compact = [[address stringByReplacingOccurrencesOfString:@":" withString:@""]
        stringByReplacingOccurrencesOfString:@"-" withString:@""];
    return [IOBluetoothDevice deviceWithAddressString:compact];
}

static BOOL bose_complete_bmap_frames(NSData *data) {
    const uint8_t *bytes = (const uint8_t *)[data bytes];
    NSUInteger len = [data length];
    NSUInteger pos = 0;
    BOOL sawFrame = NO;

    while (pos < len) {
        if (len - pos < 4) {
            return NO;
        }
        NSUInteger payloadLen = bytes[pos + 3];
        NSUInteger frameLen = 4 + payloadLen;
        if (len - pos < frameLen) {
            return NO;
        }
        sawFrame = YES;
        pos += frameLen;
    }

    return sawFrame;
}

int bose_rfcomm_send_recv(
    const char *address,
    uint8_t channel_id,
    const uint8_t *request,
    uint16_t request_len,
    uint8_t *response,
    uint16_t response_cap,
    uint16_t *response_len,
    uint32_t timeout_ms,
    char *error,
    uint16_t error_cap
) {
    @autoreleasepool {
        if (response_len != NULL) {
            *response_len = 0;
        }
        if (address == NULL || request == NULL || request_len == 0 || response == NULL || response_len == NULL) {
            bose_set_error(error, error_cap, @"invalid RFCOMM arguments");
            return -1;
        }

        NSString *address_string = [NSString stringWithUTF8String:address];
        IOBluetoothDevice *device = bose_device_for_address(address_string);
        if (device == nil) {
            bose_set_error(error, error_cap, [NSString stringWithFormat:@"Bluetooth device not found: %@", address_string]);
            return -2;
        }

        BoseRFCOMMDelegate *delegate = [[BoseRFCOMMDelegate alloc] init];
        IOBluetoothRFCOMMChannel *channel = nil;
        IOReturn result = [device openRFCOMMChannelSync:&channel
                                          withChannelID:channel_id
                                               delegate:delegate];
        if (result != kIOReturnSuccess || channel == nil) {
            bose_set_error(error, error_cap, [NSString stringWithFormat:@"open RFCOMM channel %u failed: 0x%08x", channel_id, result]);
            return -3;
        }

        [channel setDelegate:delegate];
        result = [channel writeSync:(void *)request length:request_len];
        if (result != kIOReturnSuccess) {
            [channel closeChannel];
            bose_set_error(error, error_cap, [NSString stringWithFormat:@"RFCOMM write failed: 0x%08x", result]);
            return -4;
        }

        NSDate *deadline = [NSDate dateWithTimeIntervalSinceNow:((double)timeout_ms / 1000.0)];
        NSDate *idleDeadline = nil;
        NSUInteger lastLength = 0;
        while (!delegate.closed && [[NSDate date] compare:deadline] == NSOrderedAscending) {
            @autoreleasepool {
                [[NSRunLoop currentRunLoop] runMode:NSDefaultRunLoopMode
                                         beforeDate:[NSDate dateWithTimeIntervalSinceNow:0.05]];
            }

            NSUInteger currentLength = [delegate.data length];
            if (currentLength != lastLength) {
                lastLength = currentLength;
                idleDeadline = [NSDate dateWithTimeIntervalSinceNow:0.15];
            }

            if (bose_complete_bmap_frames(delegate.data) && idleDeadline != nil && [[NSDate date] compare:idleDeadline] != NSOrderedAscending) {
                break;
            }
        }

        if (!bose_complete_bmap_frames(delegate.data)) {
            [channel closeChannel];
            bose_set_error(error, error_cap, @"RFCOMM read timed out");
            return -5;
        }

        NSUInteger len = [delegate.data length];
        if (len > response_cap) {
            len = response_cap;
        }
        memcpy(response, [delegate.data bytes], len);
        *response_len = (uint16_t)len;
        [channel closeChannel];
        return 0;
    }
}
