import * as $protobuf from "@ohos/protobufjs";
import Long = require("long");
/** Namespace muster. */
export namespace muster {

    /** Namespace v1. */
    namespace v1 {

        /** Properties of an Envelope. */
        interface IEnvelope {

            /** Envelope id */
            id?: (number|Long|null);

            /** Envelope request */
            request?: (muster.v1.IRequest|null);

            /** Envelope response */
            response?: (muster.v1.IResponse|null);

            /** Envelope event */
            event?: (muster.v1.IEvent|null);

            /** Envelope cancel */
            cancel?: (muster.v1.ICancel|null);

            /** Envelope presence */
            presence?: (Uint8Array|null);

            /** Envelope target */
            target?: (string|null);

            /** Envelope presenceEnrollment */
            presenceEnrollment?: (Uint8Array|null);
        }

        /** Represents an Envelope. */
        class Envelope implements IEnvelope {

            /**
             * Constructs a new Envelope.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.IEnvelope);

            /** Envelope id. */
            public id: (number|Long);

            /** Envelope request. */
            public request?: (muster.v1.IRequest|null);

            /** Envelope response. */
            public response?: (muster.v1.IResponse|null);

            /** Envelope event. */
            public event?: (muster.v1.IEvent|null);

            /** Envelope cancel. */
            public cancel?: (muster.v1.ICancel|null);

            /** Envelope presence. */
            public presence: Uint8Array;

            /** Envelope target. */
            public target: string;

            /** Envelope presenceEnrollment. */
            public presenceEnrollment: Uint8Array;

            /** Envelope msg. */
            public msg?: ("request"|"response"|"event"|"cancel");

            /**
             * Creates a new Envelope instance using the specified properties.
             * @param [properties] Properties to set
             * @returns Envelope instance
             */
            public static create(properties?: muster.v1.IEnvelope): muster.v1.Envelope;

            /**
             * Encodes the specified Envelope message. Does not implicitly {@link muster.v1.Envelope.verify|verify} messages.
             * @param message Envelope message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.IEnvelope, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes an Envelope message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns Envelope
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.Envelope;

            /**
             * Gets the default type url for Envelope
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a Cancel. */
        interface ICancel {
        }

        /** Represents a Cancel. */
        class Cancel implements ICancel {

            /**
             * Constructs a new Cancel.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.ICancel);

            /**
             * Creates a new Cancel instance using the specified properties.
             * @param [properties] Properties to set
             * @returns Cancel instance
             */
            public static create(properties?: muster.v1.ICancel): muster.v1.Cancel;

            /**
             * Encodes the specified Cancel message. Does not implicitly {@link muster.v1.Cancel.verify|verify} messages.
             * @param message Cancel message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.ICancel, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a Cancel message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns Cancel
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.Cancel;

            /**
             * Gets the default type url for Cancel
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of an Error. */
        interface IError {

            /** Error code */
            code?: (muster.v1.Error.Code|null);

            /** Error message */
            message?: (string|null);
        }

        /** Represents an Error. */
        class Error implements IError {

            /**
             * Constructs a new Error.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.IError);

            /** Error code. */
            public code: muster.v1.Error.Code;

            /** Error message. */
            public message: string;

            /**
             * Creates a new Error instance using the specified properties.
             * @param [properties] Properties to set
             * @returns Error instance
             */
            public static create(properties?: muster.v1.IError): muster.v1.Error;

            /**
             * Encodes the specified Error message. Does not implicitly {@link muster.v1.Error.verify|verify} messages.
             * @param message Error message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.IError, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes an Error message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns Error
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.Error;

            /**
             * Gets the default type url for Error
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        namespace Error {

            /** Code enum. */
            enum Code {
                UNKNOWN = 0,
                UNAUTHORIZED = 1,
                NOT_FOUND = 2,
                INVALID = 3,
                UNAVAILABLE = 4,
                CONFLICT = 5,
                TIMEOUT = 6,
                LOCKED_USE_REQUIRED = 7,
                BUSY = 8
            }
        }

        /** Properties of an Ack. */
        interface IAck {
        }

        /** Represents an Ack. */
        class Ack implements IAck {

            /**
             * Constructs a new Ack.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.IAck);

            /**
             * Creates a new Ack instance using the specified properties.
             * @param [properties] Properties to set
             * @returns Ack instance
             */
            public static create(properties?: muster.v1.IAck): muster.v1.Ack;

            /**
             * Encodes the specified Ack message. Does not implicitly {@link muster.v1.Ack.verify|verify} messages.
             * @param message Ack message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.IAck, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes an Ack message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns Ack
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.Ack;

            /**
             * Gets the default type url for Ack
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of an OpResult. */
        interface IOpResult {

            /** OpResult status */
            status?: (muster.v1.OpResult.Status|null);

            /** OpResult message */
            message?: (string|null);

            /** OpResult resultId */
            resultId?: (string|null);

            /** OpResult unconfirmed */
            unconfirmed?: (string|null);
        }

        /** Represents an OpResult. */
        class OpResult implements IOpResult {

            /**
             * Constructs a new OpResult.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.IOpResult);

            /** OpResult status. */
            public status: muster.v1.OpResult.Status;

            /** OpResult message. */
            public message: string;

            /** OpResult resultId. */
            public resultId: string;

            /** OpResult unconfirmed. */
            public unconfirmed: string;

            /**
             * Creates a new OpResult instance using the specified properties.
             * @param [properties] Properties to set
             * @returns OpResult instance
             */
            public static create(properties?: muster.v1.IOpResult): muster.v1.OpResult;

            /**
             * Encodes the specified OpResult message. Does not implicitly {@link muster.v1.OpResult.verify|verify} messages.
             * @param message OpResult message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.IOpResult, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes an OpResult message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns OpResult
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.OpResult;

            /**
             * Gets the default type url for OpResult
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        namespace OpResult {

            /** Status enum. */
            enum Status {
                APPLIED = 0,
                DUPLICATE = 1,
                REJECTED = 2,
                UNKNOWN = 3,
                USER_CONFIRMED = 4
            }
        }

        /** Properties of a Request. */
        interface IRequest {

            /** Request deviceInfo */
            deviceInfo?: (muster.v1.IDeviceInfoRequest|null);

            /** Request deviceWatch */
            deviceWatch?: (muster.v1.IDeviceWatchRequest|null);

            /** Request deviceSetRelay */
            deviceSetRelay?: (muster.v1.IDeviceSetRelayRequest|null);

            /** Request deviceRequestAccessibility */
            deviceRequestAccessibility?: (muster.v1.IDeviceRequestAccessibilityRequest|null);

            /** Request deviceDebug */
            deviceDebug?: (muster.v1.IDeviceDebugRequest|null);

            /** Request deviceSetCover */
            deviceSetCover?: (muster.v1.IDeviceSetCoverRequest|null);

            /** Request lockedUsePrepare */
            lockedUsePrepare?: (muster.v1.ILockedUsePrepareRequest|null);

            /** Request lockedUseExecute */
            lockedUseExecute?: (muster.v1.ILockedUseExecuteRequest|null);

            /** Request lockedUseCancel */
            lockedUseCancel?: (muster.v1.ILockedUseCancelRequest|null);

            /** Request deviceRequestAutomation */
            deviceRequestAutomation?: (muster.v1.IDeviceRequestAutomationRequest|null);

            /** Request networkStatus */
            networkStatus?: (muster.v1.INetworkStatusRequest|null);

            /** Request networkCreate */
            networkCreate?: (muster.v1.INetworkCreateRequest|null);

            /** Request networkInvite */
            networkInvite?: (muster.v1.INetworkInviteRequest|null);

            /** Request networkJoin */
            networkJoin?: (muster.v1.INetworkJoinRequest|null);

            /** Request networkPending */
            networkPending?: (muster.v1.INetworkPendingRequest|null);

            /** Request networkDecide */
            networkDecide?: (muster.v1.INetworkDecideRequest|null);

            /** Request networkRemove */
            networkRemove?: (muster.v1.INetworkRemoveRequest|null);

            /** Request networkSetScopes */
            networkSetScopes?: (muster.v1.INetworkSetScopesRequest|null);

            /** Request policyGet */
            policyGet?: (muster.v1.IPolicyGetRequest|null);

            /** Request policySet */
            policySet?: (muster.v1.IPolicySetRequest|null);

            /** Request terminalSessions */
            terminalSessions?: (muster.v1.ITerminalSessionsRequest|null);

            /** Request terminalAttach */
            terminalAttach?: (muster.v1.ITerminalAttachRequest|null);

            /** Request terminalInput */
            terminalInput?: (muster.v1.ITerminalInputRequest|null);

            /** Request terminalCreate */
            terminalCreate?: (muster.v1.ITerminalCreateRequest|null);

            /** Request terminalResize */
            terminalResize?: (muster.v1.ITerminalResizeRequest|null);

            /** Request terminalClose */
            terminalClose?: (muster.v1.ITerminalCloseRequest|null);

            /** Request terminalHistory */
            terminalHistory?: (muster.v1.ITerminalHistoryRequest|null);

            /** Request agentTerminal */
            agentTerminal?: (muster.v1.IAgentTerminalRequest|null);

            /** Request terminalRun */
            terminalRun?: (muster.v1.ITerminalRunRequest|null);

            /** Request terminalRuns */
            terminalRuns?: (muster.v1.ITerminalRunsRequest|null);

            /** Request agentList */
            agentList?: (muster.v1.IAgentListRequest|null);

            /** Request agentWatch */
            agentWatch?: (muster.v1.IAgentWatchRequest|null);

            /** Request agentMessages */
            agentMessages?: (muster.v1.IAgentMessagesRequest|null);

            /** Request agentSend */
            agentSend?: (muster.v1.IAgentSendRequest|null);

            /** Request agentRespond */
            agentRespond?: (muster.v1.IAgentRespondRequest|null);

            /** Request agentInterrupt */
            agentInterrupt?: (muster.v1.IAgentInterruptRequest|null);

            /** Request agentFiles */
            agentFiles?: (muster.v1.IAgentFilesRequest|null);

            /** Request agentFile */
            agentFile?: (muster.v1.IAgentFileRequest|null);

            /** Request agentGit */
            agentGit?: (muster.v1.IAgentGitRequest|null);

            /** Request browserCall */
            browserCall?: (muster.v1.IBrowserCallRequest|null);

            /** Request browserSetup */
            browserSetup?: (muster.v1.IBrowserSetupRequest|null);

            /** Request agentChats */
            agentChats?: (muster.v1.IAgentChatsRequest|null);

            /** Request agentChatOpen */
            agentChatOpen?: (muster.v1.IAgentChatOpenRequest|null);

            /** Request agentChatOptions */
            agentChatOptions?: (muster.v1.IAgentChatOptionsRequest|null);

            /** Request agentChatSetOptions */
            agentChatSetOptions?: (muster.v1.IAgentChatSetOptionsRequest|null);

            /** Request networkReaddress */
            networkReaddress?: (muster.v1.INetworkReaddressRequest|null);

            /** Request desktopUpdateLease */
            desktopUpdateLease?: (muster.v1.IDesktopUpdateLeaseRequest|null);

            /** Request agentInspect */
            agentInspect?: (muster.v1.IAgentInspectRequest|null);

            /** Request diagnosticsUpload */
            diagnosticsUpload?: (muster.v1.IDiagnosticsUploadRequest|null);

            /** Request lockedUseRenew */
            lockedUseRenew?: (muster.v1.ILockedUseRenewRequest|null);

            /** Request lockedUseExtend */
            lockedUseExtend?: (muster.v1.ILockedUseExtendRequest|null);

            /** Request networkRefresh */
            networkRefresh?: (muster.v1.INetworkRefreshRequest|null);

            /** Request orcaCreate */
            orcaCreate?: (muster.v1.IOrcaCreateRequest|null);

            /** Request agentComplete */
            agentComplete?: (muster.v1.IAgentCompleteRequest|null);

            /** Request devicePresence */
            devicePresence?: (muster.v1.IDevicePresenceRequest|null);

            /** Request networkPresence */
            networkPresence?: (muster.v1.INetworkPresenceRequest|null);

            /** Request deviceInstallPresence */
            deviceInstallPresence?: (muster.v1.IDeviceInstallPresenceRequest|null);

            /** Request agentSendStatus */
            agentSendStatus?: (muster.v1.IAgentSendRequest|null);

            /** Request forwardOpen */
            forwardOpen?: (muster.v1.IForwardOpenRequest|null);

            /** Request multica */
            multica?: (muster.v1.IMulticaRequest|null);

            /** Request multicaBrowse */
            multicaBrowse?: (muster.v1.IMulticaBrowseRequest|null);

            /** Request agentNativeCreate */
            agentNativeCreate?: (muster.v1.IAgentNativeCreateRequest|null);

            /** Request agentConfirmDelivered */
            agentConfirmDelivered?: (muster.v1.IAgentSendRequest|null);
        }

        /** Represents a Request. */
        class Request implements IRequest {

            /**
             * Constructs a new Request.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.IRequest);

            /** Request deviceInfo. */
            public deviceInfo?: (muster.v1.IDeviceInfoRequest|null);

            /** Request deviceWatch. */
            public deviceWatch?: (muster.v1.IDeviceWatchRequest|null);

            /** Request deviceSetRelay. */
            public deviceSetRelay?: (muster.v1.IDeviceSetRelayRequest|null);

            /** Request deviceRequestAccessibility. */
            public deviceRequestAccessibility?: (muster.v1.IDeviceRequestAccessibilityRequest|null);

            /** Request deviceDebug. */
            public deviceDebug?: (muster.v1.IDeviceDebugRequest|null);

            /** Request deviceSetCover. */
            public deviceSetCover?: (muster.v1.IDeviceSetCoverRequest|null);

            /** Request lockedUsePrepare. */
            public lockedUsePrepare?: (muster.v1.ILockedUsePrepareRequest|null);

            /** Request lockedUseExecute. */
            public lockedUseExecute?: (muster.v1.ILockedUseExecuteRequest|null);

            /** Request lockedUseCancel. */
            public lockedUseCancel?: (muster.v1.ILockedUseCancelRequest|null);

            /** Request deviceRequestAutomation. */
            public deviceRequestAutomation?: (muster.v1.IDeviceRequestAutomationRequest|null);

            /** Request networkStatus. */
            public networkStatus?: (muster.v1.INetworkStatusRequest|null);

            /** Request networkCreate. */
            public networkCreate?: (muster.v1.INetworkCreateRequest|null);

            /** Request networkInvite. */
            public networkInvite?: (muster.v1.INetworkInviteRequest|null);

            /** Request networkJoin. */
            public networkJoin?: (muster.v1.INetworkJoinRequest|null);

            /** Request networkPending. */
            public networkPending?: (muster.v1.INetworkPendingRequest|null);

            /** Request networkDecide. */
            public networkDecide?: (muster.v1.INetworkDecideRequest|null);

            /** Request networkRemove. */
            public networkRemove?: (muster.v1.INetworkRemoveRequest|null);

            /** Request networkSetScopes. */
            public networkSetScopes?: (muster.v1.INetworkSetScopesRequest|null);

            /** Request policyGet. */
            public policyGet?: (muster.v1.IPolicyGetRequest|null);

            /** Request policySet. */
            public policySet?: (muster.v1.IPolicySetRequest|null);

            /** Request terminalSessions. */
            public terminalSessions?: (muster.v1.ITerminalSessionsRequest|null);

            /** Request terminalAttach. */
            public terminalAttach?: (muster.v1.ITerminalAttachRequest|null);

            /** Request terminalInput. */
            public terminalInput?: (muster.v1.ITerminalInputRequest|null);

            /** Request terminalCreate. */
            public terminalCreate?: (muster.v1.ITerminalCreateRequest|null);

            /** Request terminalResize. */
            public terminalResize?: (muster.v1.ITerminalResizeRequest|null);

            /** Request terminalClose. */
            public terminalClose?: (muster.v1.ITerminalCloseRequest|null);

            /** Request terminalHistory. */
            public terminalHistory?: (muster.v1.ITerminalHistoryRequest|null);

            /** Request agentTerminal. */
            public agentTerminal?: (muster.v1.IAgentTerminalRequest|null);

            /** Request terminalRun. */
            public terminalRun?: (muster.v1.ITerminalRunRequest|null);

            /** Request terminalRuns. */
            public terminalRuns?: (muster.v1.ITerminalRunsRequest|null);

            /** Request agentList. */
            public agentList?: (muster.v1.IAgentListRequest|null);

            /** Request agentWatch. */
            public agentWatch?: (muster.v1.IAgentWatchRequest|null);

            /** Request agentMessages. */
            public agentMessages?: (muster.v1.IAgentMessagesRequest|null);

            /** Request agentSend. */
            public agentSend?: (muster.v1.IAgentSendRequest|null);

            /** Request agentRespond. */
            public agentRespond?: (muster.v1.IAgentRespondRequest|null);

            /** Request agentInterrupt. */
            public agentInterrupt?: (muster.v1.IAgentInterruptRequest|null);

            /** Request agentFiles. */
            public agentFiles?: (muster.v1.IAgentFilesRequest|null);

            /** Request agentFile. */
            public agentFile?: (muster.v1.IAgentFileRequest|null);

            /** Request agentGit. */
            public agentGit?: (muster.v1.IAgentGitRequest|null);

            /** Request browserCall. */
            public browserCall?: (muster.v1.IBrowserCallRequest|null);

            /** Request browserSetup. */
            public browserSetup?: (muster.v1.IBrowserSetupRequest|null);

            /** Request agentChats. */
            public agentChats?: (muster.v1.IAgentChatsRequest|null);

            /** Request agentChatOpen. */
            public agentChatOpen?: (muster.v1.IAgentChatOpenRequest|null);

            /** Request agentChatOptions. */
            public agentChatOptions?: (muster.v1.IAgentChatOptionsRequest|null);

            /** Request agentChatSetOptions. */
            public agentChatSetOptions?: (muster.v1.IAgentChatSetOptionsRequest|null);

            /** Request networkReaddress. */
            public networkReaddress?: (muster.v1.INetworkReaddressRequest|null);

            /** Request desktopUpdateLease. */
            public desktopUpdateLease?: (muster.v1.IDesktopUpdateLeaseRequest|null);

            /** Request agentInspect. */
            public agentInspect?: (muster.v1.IAgentInspectRequest|null);

            /** Request diagnosticsUpload. */
            public diagnosticsUpload?: (muster.v1.IDiagnosticsUploadRequest|null);

            /** Request lockedUseRenew. */
            public lockedUseRenew?: (muster.v1.ILockedUseRenewRequest|null);

            /** Request lockedUseExtend. */
            public lockedUseExtend?: (muster.v1.ILockedUseExtendRequest|null);

            /** Request networkRefresh. */
            public networkRefresh?: (muster.v1.INetworkRefreshRequest|null);

            /** Request orcaCreate. */
            public orcaCreate?: (muster.v1.IOrcaCreateRequest|null);

            /** Request agentComplete. */
            public agentComplete?: (muster.v1.IAgentCompleteRequest|null);

            /** Request devicePresence. */
            public devicePresence?: (muster.v1.IDevicePresenceRequest|null);

            /** Request networkPresence. */
            public networkPresence?: (muster.v1.INetworkPresenceRequest|null);

            /** Request deviceInstallPresence. */
            public deviceInstallPresence?: (muster.v1.IDeviceInstallPresenceRequest|null);

            /** Request agentSendStatus. */
            public agentSendStatus?: (muster.v1.IAgentSendRequest|null);

            /** Request forwardOpen. */
            public forwardOpen?: (muster.v1.IForwardOpenRequest|null);

            /** Request multica. */
            public multica?: (muster.v1.IMulticaRequest|null);

            /** Request multicaBrowse. */
            public multicaBrowse?: (muster.v1.IMulticaBrowseRequest|null);

            /** Request agentNativeCreate. */
            public agentNativeCreate?: (muster.v1.IAgentNativeCreateRequest|null);

            /** Request agentConfirmDelivered. */
            public agentConfirmDelivered?: (muster.v1.IAgentSendRequest|null);

            /** Request req. */
            public req?: ("deviceInfo"|"deviceWatch"|"deviceSetRelay"|"deviceRequestAccessibility"|"deviceDebug"|"deviceSetCover"|"lockedUsePrepare"|"lockedUseExecute"|"lockedUseCancel"|"deviceRequestAutomation"|"networkStatus"|"networkCreate"|"networkInvite"|"networkJoin"|"networkPending"|"networkDecide"|"networkRemove"|"networkSetScopes"|"policyGet"|"policySet"|"terminalSessions"|"terminalAttach"|"terminalInput"|"terminalCreate"|"terminalResize"|"terminalClose"|"terminalHistory"|"agentTerminal"|"terminalRun"|"terminalRuns"|"agentList"|"agentWatch"|"agentMessages"|"agentSend"|"agentRespond"|"agentInterrupt"|"agentFiles"|"agentFile"|"agentGit"|"browserCall"|"browserSetup"|"agentChats"|"agentChatOpen"|"agentChatOptions"|"agentChatSetOptions"|"networkReaddress"|"desktopUpdateLease"|"agentInspect"|"diagnosticsUpload"|"lockedUseRenew"|"lockedUseExtend"|"networkRefresh"|"orcaCreate"|"agentComplete"|"devicePresence"|"networkPresence"|"deviceInstallPresence"|"agentSendStatus"|"forwardOpen"|"multica"|"multicaBrowse"|"agentNativeCreate"|"agentConfirmDelivered");

            /**
             * Creates a new Request instance using the specified properties.
             * @param [properties] Properties to set
             * @returns Request instance
             */
            public static create(properties?: muster.v1.IRequest): muster.v1.Request;

            /**
             * Encodes the specified Request message. Does not implicitly {@link muster.v1.Request.verify|verify} messages.
             * @param message Request message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.IRequest, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a Request message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns Request
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.Request;

            /**
             * Gets the default type url for Request
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of an OrcaCreateRequest. */
        interface IOrcaCreateRequest {

            /** OrcaCreateRequest opId */
            opId?: (string|null);

            /** OrcaCreateRequest directory */
            directory?: (string|null);

            /** OrcaCreateRequest agent */
            agent?: (string|null);

            /** OrcaCreateRequest env */
            env?: ({ [k: string]: string }|null);
        }

        /** Represents an OrcaCreateRequest. */
        class OrcaCreateRequest implements IOrcaCreateRequest {

            /**
             * Constructs a new OrcaCreateRequest.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.IOrcaCreateRequest);

            /** OrcaCreateRequest opId. */
            public opId: string;

            /** OrcaCreateRequest directory. */
            public directory: string;

            /** OrcaCreateRequest agent. */
            public agent: string;

            /** OrcaCreateRequest env. */
            public env: { [k: string]: string };

            /**
             * Creates a new OrcaCreateRequest instance using the specified properties.
             * @param [properties] Properties to set
             * @returns OrcaCreateRequest instance
             */
            public static create(properties?: muster.v1.IOrcaCreateRequest): muster.v1.OrcaCreateRequest;

            /**
             * Encodes the specified OrcaCreateRequest message. Does not implicitly {@link muster.v1.OrcaCreateRequest.verify|verify} messages.
             * @param message OrcaCreateRequest message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.IOrcaCreateRequest, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes an OrcaCreateRequest message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns OrcaCreateRequest
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.OrcaCreateRequest;

            /**
             * Gets the default type url for OrcaCreateRequest
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a Response. */
        interface IResponse {

            /** Response error */
            error?: (muster.v1.IError|null);

            /** Response ack */
            ack?: (muster.v1.IAck|null);

            /** Response opResult */
            opResult?: (muster.v1.IOpResult|null);

            /** Response deviceInfo */
            deviceInfo?: (muster.v1.IDeviceInfo|null);

            /** Response debugText */
            debugText?: (muster.v1.IDebugText|null);

            /** Response lockedUseChallenge */
            lockedUseChallenge?: (muster.v1.ILockedUseChallenge|null);

            /** Response networkStatus */
            networkStatus?: (muster.v1.INetworkStatus|null);

            /** Response inviteText */
            inviteText?: (muster.v1.IInviteText|null);

            /** Response pendingJoins */
            pendingJoins?: (muster.v1.IPendingJoins|null);

            /** Response policy */
            policy?: (muster.v1.IPolicy|null);

            /** Response terminalSessions */
            terminalSessions?: (muster.v1.ITerminalSessionList|null);

            /** Response terminalSession */
            terminalSession?: (muster.v1.ITerminalSession|null);

            /** Response terminalLines */
            terminalLines?: (muster.v1.ITerminalLines|null);

            /** Response terminalRuns */
            terminalRuns?: (muster.v1.ITerminalRunList|null);

            /** Response agentSessions */
            agentSessions?: (muster.v1.IAgentSessionPage|null);

            /** Response agentMessages */
            agentMessages?: (muster.v1.IAgentMessagePage|null);

            /** Response agentSession */
            agentSession?: (muster.v1.IAgentSession|null);

            /** Response fileList */
            fileList?: (muster.v1.IFileList|null);

            /** Response fileContent */
            fileContent?: (muster.v1.IFileContent|null);

            /** Response gitLog */
            gitLog?: (muster.v1.IGitLog|null);

            /** Response browserStatus */
            browserStatus?: (muster.v1.IBrowserStatus|null);

            /** Response agentChats */
            agentChats?: (muster.v1.IAgentChatCatalog|null);

            /** Response agentChatOptions */
            agentChatOptions?: (muster.v1.IAgentChatOptions|null);

            /** Response diagnosticsSaved */
            diagnosticsSaved?: (muster.v1.IDiagnosticsSaved|null);

            /** Response agentCompletion */
            agentCompletion?: (muster.v1.IAgentCompletion|null);

            /** Response presenceInfo */
            presenceInfo?: (muster.v1.IPresenceInfo|null);

            /** Response multica */
            multica?: (muster.v1.IMulticaStatus|null);

            /** Response multicaPage */
            multicaPage?: (muster.v1.IMulticaPage|null);
        }

        /** Represents a Response. */
        class Response implements IResponse {

            /**
             * Constructs a new Response.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.IResponse);

            /** Response error. */
            public error?: (muster.v1.IError|null);

            /** Response ack. */
            public ack?: (muster.v1.IAck|null);

            /** Response opResult. */
            public opResult?: (muster.v1.IOpResult|null);

            /** Response deviceInfo. */
            public deviceInfo?: (muster.v1.IDeviceInfo|null);

            /** Response debugText. */
            public debugText?: (muster.v1.IDebugText|null);

            /** Response lockedUseChallenge. */
            public lockedUseChallenge?: (muster.v1.ILockedUseChallenge|null);

            /** Response networkStatus. */
            public networkStatus?: (muster.v1.INetworkStatus|null);

            /** Response inviteText. */
            public inviteText?: (muster.v1.IInviteText|null);

            /** Response pendingJoins. */
            public pendingJoins?: (muster.v1.IPendingJoins|null);

            /** Response policy. */
            public policy?: (muster.v1.IPolicy|null);

            /** Response terminalSessions. */
            public terminalSessions?: (muster.v1.ITerminalSessionList|null);

            /** Response terminalSession. */
            public terminalSession?: (muster.v1.ITerminalSession|null);

            /** Response terminalLines. */
            public terminalLines?: (muster.v1.ITerminalLines|null);

            /** Response terminalRuns. */
            public terminalRuns?: (muster.v1.ITerminalRunList|null);

            /** Response agentSessions. */
            public agentSessions?: (muster.v1.IAgentSessionPage|null);

            /** Response agentMessages. */
            public agentMessages?: (muster.v1.IAgentMessagePage|null);

            /** Response agentSession. */
            public agentSession?: (muster.v1.IAgentSession|null);

            /** Response fileList. */
            public fileList?: (muster.v1.IFileList|null);

            /** Response fileContent. */
            public fileContent?: (muster.v1.IFileContent|null);

            /** Response gitLog. */
            public gitLog?: (muster.v1.IGitLog|null);

            /** Response browserStatus. */
            public browserStatus?: (muster.v1.IBrowserStatus|null);

            /** Response agentChats. */
            public agentChats?: (muster.v1.IAgentChatCatalog|null);

            /** Response agentChatOptions. */
            public agentChatOptions?: (muster.v1.IAgentChatOptions|null);

            /** Response diagnosticsSaved. */
            public diagnosticsSaved?: (muster.v1.IDiagnosticsSaved|null);

            /** Response agentCompletion. */
            public agentCompletion?: (muster.v1.IAgentCompletion|null);

            /** Response presenceInfo. */
            public presenceInfo?: (muster.v1.IPresenceInfo|null);

            /** Response multica. */
            public multica?: (muster.v1.IMulticaStatus|null);

            /** Response multicaPage. */
            public multicaPage?: (muster.v1.IMulticaPage|null);

            /** Response resp. */
            public resp?: ("error"|"ack"|"opResult"|"deviceInfo"|"debugText"|"lockedUseChallenge"|"networkStatus"|"inviteText"|"pendingJoins"|"policy"|"terminalSessions"|"terminalSession"|"terminalLines"|"terminalRuns"|"agentSessions"|"agentMessages"|"agentSession"|"fileList"|"fileContent"|"gitLog"|"browserStatus"|"agentChats"|"agentChatOptions"|"diagnosticsSaved"|"agentCompletion"|"presenceInfo"|"multica"|"multicaPage");

            /**
             * Creates a new Response instance using the specified properties.
             * @param [properties] Properties to set
             * @returns Response instance
             */
            public static create(properties?: muster.v1.IResponse): muster.v1.Response;

            /**
             * Encodes the specified Response message. Does not implicitly {@link muster.v1.Response.verify|verify} messages.
             * @param message Response message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.IResponse, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a Response message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns Response
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.Response;

            /**
             * Gets the default type url for Response
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of an Event. */
        interface IEvent {

            /** Event deviceState */
            deviceState?: (muster.v1.IDeviceState|null);

            /** Event networkStatus */
            networkStatus?: (muster.v1.INetworkStatus|null);

            /** Event policy */
            policy?: (muster.v1.IPolicy|null);

            /** Event joinProgress */
            joinProgress?: (muster.v1.IJoinProgress|null);

            /** Event terminalSessions */
            terminalSessions?: (muster.v1.ITerminalSessionList|null);

            /** Event terminalFrame */
            terminalFrame?: (muster.v1.ITerminalFrame|null);

            /** Event agentEvent */
            agentEvent?: (muster.v1.IAgentEvent|null);

            /** Event desktopUpdateReady */
            desktopUpdateReady?: (muster.v1.IAck|null);

            /** Event forwardOpened */
            forwardOpened?: (muster.v1.IForwardOpened|null);
        }

        /** Represents an Event. */
        class Event implements IEvent {

            /**
             * Constructs a new Event.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.IEvent);

            /** Event deviceState. */
            public deviceState?: (muster.v1.IDeviceState|null);

            /** Event networkStatus. */
            public networkStatus?: (muster.v1.INetworkStatus|null);

            /** Event policy. */
            public policy?: (muster.v1.IPolicy|null);

            /** Event joinProgress. */
            public joinProgress?: (muster.v1.IJoinProgress|null);

            /** Event terminalSessions. */
            public terminalSessions?: (muster.v1.ITerminalSessionList|null);

            /** Event terminalFrame. */
            public terminalFrame?: (muster.v1.ITerminalFrame|null);

            /** Event agentEvent. */
            public agentEvent?: (muster.v1.IAgentEvent|null);

            /** Event desktopUpdateReady. */
            public desktopUpdateReady?: (muster.v1.IAck|null);

            /** Event forwardOpened. */
            public forwardOpened?: (muster.v1.IForwardOpened|null);

            /** Event ev. */
            public ev?: ("deviceState"|"networkStatus"|"policy"|"joinProgress"|"terminalSessions"|"terminalFrame"|"agentEvent"|"desktopUpdateReady"|"forwardOpened");

            /**
             * Creates a new Event instance using the specified properties.
             * @param [properties] Properties to set
             * @returns Event instance
             */
            public static create(properties?: muster.v1.IEvent): muster.v1.Event;

            /**
             * Encodes the specified Event message. Does not implicitly {@link muster.v1.Event.verify|verify} messages.
             * @param message Event message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.IEvent, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes an Event message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns Event
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.Event;

            /**
             * Gets the default type url for Event
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a DeviceInfoRequest. */
        interface IDeviceInfoRequest {
        }

        /** Represents a DeviceInfoRequest. */
        class DeviceInfoRequest implements IDeviceInfoRequest {

            /**
             * Constructs a new DeviceInfoRequest.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.IDeviceInfoRequest);

            /**
             * Creates a new DeviceInfoRequest instance using the specified properties.
             * @param [properties] Properties to set
             * @returns DeviceInfoRequest instance
             */
            public static create(properties?: muster.v1.IDeviceInfoRequest): muster.v1.DeviceInfoRequest;

            /**
             * Encodes the specified DeviceInfoRequest message. Does not implicitly {@link muster.v1.DeviceInfoRequest.verify|verify} messages.
             * @param message DeviceInfoRequest message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.IDeviceInfoRequest, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a DeviceInfoRequest message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns DeviceInfoRequest
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.DeviceInfoRequest;

            /**
             * Gets the default type url for DeviceInfoRequest
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a DesktopUpdateLeaseRequest. */
        interface IDesktopUpdateLeaseRequest {
        }

        /** Represents a DesktopUpdateLeaseRequest. */
        class DesktopUpdateLeaseRequest implements IDesktopUpdateLeaseRequest {

            /**
             * Constructs a new DesktopUpdateLeaseRequest.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.IDesktopUpdateLeaseRequest);

            /**
             * Creates a new DesktopUpdateLeaseRequest instance using the specified properties.
             * @param [properties] Properties to set
             * @returns DesktopUpdateLeaseRequest instance
             */
            public static create(properties?: muster.v1.IDesktopUpdateLeaseRequest): muster.v1.DesktopUpdateLeaseRequest;

            /**
             * Encodes the specified DesktopUpdateLeaseRequest message. Does not implicitly {@link muster.v1.DesktopUpdateLeaseRequest.verify|verify} messages.
             * @param message DesktopUpdateLeaseRequest message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.IDesktopUpdateLeaseRequest, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a DesktopUpdateLeaseRequest message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns DesktopUpdateLeaseRequest
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.DesktopUpdateLeaseRequest;

            /**
             * Gets the default type url for DesktopUpdateLeaseRequest
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a NetworkRefreshRequest. */
        interface INetworkRefreshRequest {
        }

        /** Represents a NetworkRefreshRequest. */
        class NetworkRefreshRequest implements INetworkRefreshRequest {

            /**
             * Constructs a new NetworkRefreshRequest.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.INetworkRefreshRequest);

            /**
             * Creates a new NetworkRefreshRequest instance using the specified properties.
             * @param [properties] Properties to set
             * @returns NetworkRefreshRequest instance
             */
            public static create(properties?: muster.v1.INetworkRefreshRequest): muster.v1.NetworkRefreshRequest;

            /**
             * Encodes the specified NetworkRefreshRequest message. Does not implicitly {@link muster.v1.NetworkRefreshRequest.verify|verify} messages.
             * @param message NetworkRefreshRequest message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.INetworkRefreshRequest, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a NetworkRefreshRequest message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns NetworkRefreshRequest
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.NetworkRefreshRequest;

            /**
             * Gets the default type url for NetworkRefreshRequest
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a NetworkReaddressRequest. */
        interface INetworkReaddressRequest {

            /** NetworkReaddressRequest vnetCidr */
            vnetCidr?: (string|null);

            /** NetworkReaddressRequest addresses */
            addresses?: ({ [k: string]: string }|null);
        }

        /** Represents a NetworkReaddressRequest. */
        class NetworkReaddressRequest implements INetworkReaddressRequest {

            /**
             * Constructs a new NetworkReaddressRequest.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.INetworkReaddressRequest);

            /** NetworkReaddressRequest vnetCidr. */
            public vnetCidr: string;

            /** NetworkReaddressRequest addresses. */
            public addresses: { [k: string]: string };

            /**
             * Creates a new NetworkReaddressRequest instance using the specified properties.
             * @param [properties] Properties to set
             * @returns NetworkReaddressRequest instance
             */
            public static create(properties?: muster.v1.INetworkReaddressRequest): muster.v1.NetworkReaddressRequest;

            /**
             * Encodes the specified NetworkReaddressRequest message. Does not implicitly {@link muster.v1.NetworkReaddressRequest.verify|verify} messages.
             * @param message NetworkReaddressRequest message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.INetworkReaddressRequest, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a NetworkReaddressRequest message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns NetworkReaddressRequest
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.NetworkReaddressRequest;

            /**
             * Gets the default type url for NetworkReaddressRequest
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a DeviceWatchRequest. */
        interface IDeviceWatchRequest {
        }

        /** Represents a DeviceWatchRequest. */
        class DeviceWatchRequest implements IDeviceWatchRequest {

            /**
             * Constructs a new DeviceWatchRequest.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.IDeviceWatchRequest);

            /**
             * Creates a new DeviceWatchRequest instance using the specified properties.
             * @param [properties] Properties to set
             * @returns DeviceWatchRequest instance
             */
            public static create(properties?: muster.v1.IDeviceWatchRequest): muster.v1.DeviceWatchRequest;

            /**
             * Encodes the specified DeviceWatchRequest message. Does not implicitly {@link muster.v1.DeviceWatchRequest.verify|verify} messages.
             * @param message DeviceWatchRequest message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.IDeviceWatchRequest, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a DeviceWatchRequest message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns DeviceWatchRequest
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.DeviceWatchRequest;

            /**
             * Gets the default type url for DeviceWatchRequest
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a DeviceInfo. */
        interface IDeviceInfo {

            /** DeviceInfo deviceId */
            deviceId?: (string|null);

            /** DeviceInfo fingerprint */
            fingerprint?: (string|null);

            /** DeviceInfo name */
            name?: (string|null);

            /** DeviceInfo kind */
            kind?: (string|null);

            /** DeviceInfo version */
            version?: (string|null);

            /** DeviceInfo platform */
            platform?: (string|null);

            /** DeviceInfo state */
            state?: (muster.v1.IDeviceState|null);

            /** DeviceInfo services */
            services?: (string[]|null);

            /** DeviceInfo relay */
            relay?: (string|null);

            /** DeviceInfo keyBackend */
            keyBackend?: (string|null);

            /** DeviceInfo keyLabel */
            keyLabel?: (string|null);

            /** DeviceInfo commit */
            commit?: (string|null);
        }

        /** Represents a DeviceInfo. */
        class DeviceInfo implements IDeviceInfo {

            /**
             * Constructs a new DeviceInfo.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.IDeviceInfo);

            /** DeviceInfo deviceId. */
            public deviceId: string;

            /** DeviceInfo fingerprint. */
            public fingerprint: string;

            /** DeviceInfo name. */
            public name: string;

            /** DeviceInfo kind. */
            public kind: string;

            /** DeviceInfo version. */
            public version: string;

            /** DeviceInfo platform. */
            public platform: string;

            /** DeviceInfo state. */
            public state?: (muster.v1.IDeviceState|null);

            /** DeviceInfo services. */
            public services: string[];

            /** DeviceInfo relay. */
            public relay: string;

            /** DeviceInfo keyBackend. */
            public keyBackend: string;

            /** DeviceInfo keyLabel. */
            public keyLabel: string;

            /** DeviceInfo commit. */
            public commit: string;

            /**
             * Creates a new DeviceInfo instance using the specified properties.
             * @param [properties] Properties to set
             * @returns DeviceInfo instance
             */
            public static create(properties?: muster.v1.IDeviceInfo): muster.v1.DeviceInfo;

            /**
             * Encodes the specified DeviceInfo message. Does not implicitly {@link muster.v1.DeviceInfo.verify|verify} messages.
             * @param message DeviceInfo message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.IDeviceInfo, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a DeviceInfo message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns DeviceInfo
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.DeviceInfo;

            /**
             * Gets the default type url for DeviceInfo
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a DeviceRequestAccessibilityRequest. */
        interface IDeviceRequestAccessibilityRequest {
        }

        /** Represents a DeviceRequestAccessibilityRequest. */
        class DeviceRequestAccessibilityRequest implements IDeviceRequestAccessibilityRequest {

            /**
             * Constructs a new DeviceRequestAccessibilityRequest.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.IDeviceRequestAccessibilityRequest);

            /**
             * Creates a new DeviceRequestAccessibilityRequest instance using the specified properties.
             * @param [properties] Properties to set
             * @returns DeviceRequestAccessibilityRequest instance
             */
            public static create(properties?: muster.v1.IDeviceRequestAccessibilityRequest): muster.v1.DeviceRequestAccessibilityRequest;

            /**
             * Encodes the specified DeviceRequestAccessibilityRequest message. Does not implicitly {@link muster.v1.DeviceRequestAccessibilityRequest.verify|verify} messages.
             * @param message DeviceRequestAccessibilityRequest message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.IDeviceRequestAccessibilityRequest, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a DeviceRequestAccessibilityRequest message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns DeviceRequestAccessibilityRequest
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.DeviceRequestAccessibilityRequest;

            /**
             * Gets the default type url for DeviceRequestAccessibilityRequest
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a DeviceRequestAutomationRequest. */
        interface IDeviceRequestAutomationRequest {
        }

        /** Represents a DeviceRequestAutomationRequest. */
        class DeviceRequestAutomationRequest implements IDeviceRequestAutomationRequest {

            /**
             * Constructs a new DeviceRequestAutomationRequest.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.IDeviceRequestAutomationRequest);

            /**
             * Creates a new DeviceRequestAutomationRequest instance using the specified properties.
             * @param [properties] Properties to set
             * @returns DeviceRequestAutomationRequest instance
             */
            public static create(properties?: muster.v1.IDeviceRequestAutomationRequest): muster.v1.DeviceRequestAutomationRequest;

            /**
             * Encodes the specified DeviceRequestAutomationRequest message. Does not implicitly {@link muster.v1.DeviceRequestAutomationRequest.verify|verify} messages.
             * @param message DeviceRequestAutomationRequest message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.IDeviceRequestAutomationRequest, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a DeviceRequestAutomationRequest message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns DeviceRequestAutomationRequest
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.DeviceRequestAutomationRequest;

            /**
             * Gets the default type url for DeviceRequestAutomationRequest
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a DeviceDebugRequest. */
        interface IDeviceDebugRequest {

            /** DeviceDebugRequest what */
            what?: (string|null);

            /** DeviceDebugRequest arg */
            arg?: (string|null);
        }

        /** Represents a DeviceDebugRequest. */
        class DeviceDebugRequest implements IDeviceDebugRequest {

            /**
             * Constructs a new DeviceDebugRequest.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.IDeviceDebugRequest);

            /** DeviceDebugRequest what. */
            public what: string;

            /** DeviceDebugRequest arg. */
            public arg: string;

            /**
             * Creates a new DeviceDebugRequest instance using the specified properties.
             * @param [properties] Properties to set
             * @returns DeviceDebugRequest instance
             */
            public static create(properties?: muster.v1.IDeviceDebugRequest): muster.v1.DeviceDebugRequest;

            /**
             * Encodes the specified DeviceDebugRequest message. Does not implicitly {@link muster.v1.DeviceDebugRequest.verify|verify} messages.
             * @param message DeviceDebugRequest message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.IDeviceDebugRequest, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a DeviceDebugRequest message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns DeviceDebugRequest
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.DeviceDebugRequest;

            /**
             * Gets the default type url for DeviceDebugRequest
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a DebugText. */
        interface IDebugText {

            /** DebugText text */
            text?: (string|null);
        }

        /** Represents a DebugText. */
        class DebugText implements IDebugText {

            /**
             * Constructs a new DebugText.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.IDebugText);

            /** DebugText text. */
            public text: string;

            /**
             * Creates a new DebugText instance using the specified properties.
             * @param [properties] Properties to set
             * @returns DebugText instance
             */
            public static create(properties?: muster.v1.IDebugText): muster.v1.DebugText;

            /**
             * Encodes the specified DebugText message. Does not implicitly {@link muster.v1.DebugText.verify|verify} messages.
             * @param message DebugText message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.IDebugText, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a DebugText message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns DebugText
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.DebugText;

            /**
             * Gets the default type url for DebugText
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a DeviceSetRelayRequest. */
        interface IDeviceSetRelayRequest {

            /** DeviceSetRelayRequest relay */
            relay?: (string|null);
        }

        /** Represents a DeviceSetRelayRequest. */
        class DeviceSetRelayRequest implements IDeviceSetRelayRequest {

            /**
             * Constructs a new DeviceSetRelayRequest.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.IDeviceSetRelayRequest);

            /** DeviceSetRelayRequest relay. */
            public relay: string;

            /**
             * Creates a new DeviceSetRelayRequest instance using the specified properties.
             * @param [properties] Properties to set
             * @returns DeviceSetRelayRequest instance
             */
            public static create(properties?: muster.v1.IDeviceSetRelayRequest): muster.v1.DeviceSetRelayRequest;

            /**
             * Encodes the specified DeviceSetRelayRequest message. Does not implicitly {@link muster.v1.DeviceSetRelayRequest.verify|verify} messages.
             * @param message DeviceSetRelayRequest message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.IDeviceSetRelayRequest, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a DeviceSetRelayRequest message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns DeviceSetRelayRequest
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.DeviceSetRelayRequest;

            /**
             * Gets the default type url for DeviceSetRelayRequest
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a DeviceState. */
        interface IDeviceState {

            /** DeviceState locked */
            locked?: (boolean|null);

            /** DeviceState onConsole */
            onConsole?: (boolean|null);

            /** DeviceState vip */
            vip?: (string|null);

            /** DeviceState vnetUp */
            vnetUp?: (boolean|null);

            /** DeviceState relay */
            relay?: (string|null);

            /** DeviceState uptimeSecs */
            uptimeSecs?: (number|Long|null);

            /** DeviceState accessibility */
            accessibility?: (string|null);

            /** DeviceState covered */
            covered?: (boolean|null);

            /** DeviceState iterm2Automation */
            iterm2Automation?: (string|null);

            /** DeviceState iterm2ApiAvailable */
            iterm2ApiAvailable?: (boolean|null);

            /** DeviceState lockedUseSetup */
            lockedUseSetup?: (string|null);

            /** DeviceState terminalSource */
            terminalSource?: (string|null);

            /** DeviceState manualLockedUseOwner */
            manualLockedUseOwner?: (string|null);

            /** DeviceState manualLockedUseNonce */
            manualLockedUseNonce?: (Uint8Array|null);

            /** DeviceState manualLockedUseActive */
            manualLockedUseActive?: (boolean|null);

            /** DeviceState ghosttyAutomation */
            ghosttyAutomation?: (string|null);
        }

        /** Represents a DeviceState. */
        class DeviceState implements IDeviceState {

            /**
             * Constructs a new DeviceState.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.IDeviceState);

            /** DeviceState locked. */
            public locked: boolean;

            /** DeviceState onConsole. */
            public onConsole: boolean;

            /** DeviceState vip. */
            public vip: string;

            /** DeviceState vnetUp. */
            public vnetUp: boolean;

            /** DeviceState relay. */
            public relay: string;

            /** DeviceState uptimeSecs. */
            public uptimeSecs: (number|Long);

            /** DeviceState accessibility. */
            public accessibility: string;

            /** DeviceState covered. */
            public covered: boolean;

            /** DeviceState iterm2Automation. */
            public iterm2Automation: string;

            /** DeviceState iterm2ApiAvailable. */
            public iterm2ApiAvailable: boolean;

            /** DeviceState lockedUseSetup. */
            public lockedUseSetup: string;

            /** DeviceState terminalSource. */
            public terminalSource: string;

            /** DeviceState manualLockedUseOwner. */
            public manualLockedUseOwner: string;

            /** DeviceState manualLockedUseNonce. */
            public manualLockedUseNonce: Uint8Array;

            /** DeviceState manualLockedUseActive. */
            public manualLockedUseActive: boolean;

            /** DeviceState ghosttyAutomation. */
            public ghosttyAutomation: string;

            /**
             * Creates a new DeviceState instance using the specified properties.
             * @param [properties] Properties to set
             * @returns DeviceState instance
             */
            public static create(properties?: muster.v1.IDeviceState): muster.v1.DeviceState;

            /**
             * Encodes the specified DeviceState message. Does not implicitly {@link muster.v1.DeviceState.verify|verify} messages.
             * @param message DeviceState message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.IDeviceState, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a DeviceState message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns DeviceState
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.DeviceState;

            /**
             * Gets the default type url for DeviceState
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a DeviceSetCoverRequest. */
        interface IDeviceSetCoverRequest {

            /** DeviceSetCoverRequest on */
            on?: (boolean|null);
        }

        /** Represents a DeviceSetCoverRequest. */
        class DeviceSetCoverRequest implements IDeviceSetCoverRequest {

            /**
             * Constructs a new DeviceSetCoverRequest.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.IDeviceSetCoverRequest);

            /** DeviceSetCoverRequest on. */
            public on: boolean;

            /**
             * Creates a new DeviceSetCoverRequest instance using the specified properties.
             * @param [properties] Properties to set
             * @returns DeviceSetCoverRequest instance
             */
            public static create(properties?: muster.v1.IDeviceSetCoverRequest): muster.v1.DeviceSetCoverRequest;

            /**
             * Encodes the specified DeviceSetCoverRequest message. Does not implicitly {@link muster.v1.DeviceSetCoverRequest.verify|verify} messages.
             * @param message DeviceSetCoverRequest message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.IDeviceSetCoverRequest, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a DeviceSetCoverRequest message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns DeviceSetCoverRequest
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.DeviceSetCoverRequest;

            /**
             * Gets the default type url for DeviceSetCoverRequest
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a LockedUseOperation. */
        interface ILockedUseOperation {

            /** LockedUseOperation send */
            send?: (muster.v1.IAgentSendRequest|null);

            /** LockedUseOperation respond */
            respond?: (muster.v1.IAgentRespondRequest|null);

            /** LockedUseOperation interrupt */
            interrupt?: (muster.v1.IAgentInterruptRequest|null);

            /** LockedUseOperation inspect */
            inspect?: (muster.v1.IAgentInspectRequest|null);

            /** LockedUseOperation terminal */
            terminal?: (muster.v1.ITerminalLeaseRequest|null);

            /** LockedUseOperation chatLoad */
            chatLoad?: (muster.v1.IAgentChatsRequest|null);

            /** LockedUseOperation chatOpen */
            chatOpen?: (muster.v1.IAgentChatOpenRequest|null);

            /** LockedUseOperation chatOptions */
            chatOptions?: (muster.v1.IAgentChatOptionsRequest|null);

            /** LockedUseOperation chatSetOptions */
            chatSetOptions?: (muster.v1.IAgentChatSetOptionsRequest|null);

            /** LockedUseOperation device */
            device?: (muster.v1.IDeviceLeaseRequest|null);

            /** LockedUseOperation complete */
            complete?: (muster.v1.IAgentCompleteRequest|null);

            /** LockedUseOperation terminalCreate */
            terminalCreate?: (muster.v1.ITerminalCreateRequest|null);

            /** LockedUseOperation providerQueue */
            providerQueue?: (string|null);
        }

        /** Represents a LockedUseOperation. */
        class LockedUseOperation implements ILockedUseOperation {

            /**
             * Constructs a new LockedUseOperation.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.ILockedUseOperation);

            /** LockedUseOperation send. */
            public send?: (muster.v1.IAgentSendRequest|null);

            /** LockedUseOperation respond. */
            public respond?: (muster.v1.IAgentRespondRequest|null);

            /** LockedUseOperation interrupt. */
            public interrupt?: (muster.v1.IAgentInterruptRequest|null);

            /** LockedUseOperation inspect. */
            public inspect?: (muster.v1.IAgentInspectRequest|null);

            /** LockedUseOperation terminal. */
            public terminal?: (muster.v1.ITerminalLeaseRequest|null);

            /** LockedUseOperation chatLoad. */
            public chatLoad?: (muster.v1.IAgentChatsRequest|null);

            /** LockedUseOperation chatOpen. */
            public chatOpen?: (muster.v1.IAgentChatOpenRequest|null);

            /** LockedUseOperation chatOptions. */
            public chatOptions?: (muster.v1.IAgentChatOptionsRequest|null);

            /** LockedUseOperation chatSetOptions. */
            public chatSetOptions?: (muster.v1.IAgentChatSetOptionsRequest|null);

            /** LockedUseOperation device. */
            public device?: (muster.v1.IDeviceLeaseRequest|null);

            /** LockedUseOperation complete. */
            public complete?: (muster.v1.IAgentCompleteRequest|null);

            /** LockedUseOperation terminalCreate. */
            public terminalCreate?: (muster.v1.ITerminalCreateRequest|null);

            /** LockedUseOperation providerQueue. */
            public providerQueue: string;

            /** LockedUseOperation operation. */
            public operation?: ("send"|"respond"|"interrupt"|"inspect"|"terminal"|"chatLoad"|"chatOpen"|"chatOptions"|"chatSetOptions"|"device"|"complete"|"terminalCreate");

            /**
             * Creates a new LockedUseOperation instance using the specified properties.
             * @param [properties] Properties to set
             * @returns LockedUseOperation instance
             */
            public static create(properties?: muster.v1.ILockedUseOperation): muster.v1.LockedUseOperation;

            /**
             * Encodes the specified LockedUseOperation message. Does not implicitly {@link muster.v1.LockedUseOperation.verify|verify} messages.
             * @param message LockedUseOperation message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.ILockedUseOperation, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a LockedUseOperation message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns LockedUseOperation
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.LockedUseOperation;

            /**
             * Gets the default type url for LockedUseOperation
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of an AgentInspectRequest. */
        interface IAgentInspectRequest {

            /** AgentInspectRequest session */
            session?: (string|null);

            /** AgentInspectRequest opId */
            opId?: (string|null);
        }

        /** Represents an AgentInspectRequest. */
        class AgentInspectRequest implements IAgentInspectRequest {

            /**
             * Constructs a new AgentInspectRequest.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.IAgentInspectRequest);

            /** AgentInspectRequest session. */
            public session: string;

            /** AgentInspectRequest opId. */
            public opId: string;

            /**
             * Creates a new AgentInspectRequest instance using the specified properties.
             * @param [properties] Properties to set
             * @returns AgentInspectRequest instance
             */
            public static create(properties?: muster.v1.IAgentInspectRequest): muster.v1.AgentInspectRequest;

            /**
             * Encodes the specified AgentInspectRequest message. Does not implicitly {@link muster.v1.AgentInspectRequest.verify|verify} messages.
             * @param message AgentInspectRequest message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.IAgentInspectRequest, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes an AgentInspectRequest message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns AgentInspectRequest
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.AgentInspectRequest;

            /**
             * Gets the default type url for AgentInspectRequest
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of an AgentCompleteRequest. */
        interface IAgentCompleteRequest {

            /** AgentCompleteRequest session */
            session?: (string|null);

            /** AgentCompleteRequest opId */
            opId?: (string|null);
        }

        /** Represents an AgentCompleteRequest. */
        class AgentCompleteRequest implements IAgentCompleteRequest {

            /**
             * Constructs a new AgentCompleteRequest.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.IAgentCompleteRequest);

            /** AgentCompleteRequest session. */
            public session: string;

            /** AgentCompleteRequest opId. */
            public opId: string;

            /**
             * Creates a new AgentCompleteRequest instance using the specified properties.
             * @param [properties] Properties to set
             * @returns AgentCompleteRequest instance
             */
            public static create(properties?: muster.v1.IAgentCompleteRequest): muster.v1.AgentCompleteRequest;

            /**
             * Encodes the specified AgentCompleteRequest message. Does not implicitly {@link muster.v1.AgentCompleteRequest.verify|verify} messages.
             * @param message AgentCompleteRequest message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.IAgentCompleteRequest, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes an AgentCompleteRequest message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns AgentCompleteRequest
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.AgentCompleteRequest;

            /**
             * Gets the default type url for AgentCompleteRequest
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of an AgentCompletion. */
        interface IAgentCompletion {

            /** AgentCompletion session */
            session?: (string|null);

            /** AgentCompletion text */
            text?: (string|null);
        }

        /** Represents an AgentCompletion. */
        class AgentCompletion implements IAgentCompletion {

            /**
             * Constructs a new AgentCompletion.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.IAgentCompletion);

            /** AgentCompletion session. */
            public session: string;

            /** AgentCompletion text. */
            public text: string;

            /**
             * Creates a new AgentCompletion instance using the specified properties.
             * @param [properties] Properties to set
             * @returns AgentCompletion instance
             */
            public static create(properties?: muster.v1.IAgentCompletion): muster.v1.AgentCompletion;

            /**
             * Encodes the specified AgentCompletion message. Does not implicitly {@link muster.v1.AgentCompletion.verify|verify} messages.
             * @param message AgentCompletion message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.IAgentCompletion, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes an AgentCompletion message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns AgentCompletion
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.AgentCompletion;

            /**
             * Gets the default type url for AgentCompletion
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a DiagnosticsUploadRequest. */
        interface IDiagnosticsUploadRequest {

            /** DiagnosticsUploadRequest kind */
            kind?: (string|null);

            /** DiagnosticsUploadRequest data */
            data?: (Uint8Array|null);
        }

        /** Represents a DiagnosticsUploadRequest. */
        class DiagnosticsUploadRequest implements IDiagnosticsUploadRequest {

            /**
             * Constructs a new DiagnosticsUploadRequest.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.IDiagnosticsUploadRequest);

            /** DiagnosticsUploadRequest kind. */
            public kind: string;

            /** DiagnosticsUploadRequest data. */
            public data: Uint8Array;

            /**
             * Creates a new DiagnosticsUploadRequest instance using the specified properties.
             * @param [properties] Properties to set
             * @returns DiagnosticsUploadRequest instance
             */
            public static create(properties?: muster.v1.IDiagnosticsUploadRequest): muster.v1.DiagnosticsUploadRequest;

            /**
             * Encodes the specified DiagnosticsUploadRequest message. Does not implicitly {@link muster.v1.DiagnosticsUploadRequest.verify|verify} messages.
             * @param message DiagnosticsUploadRequest message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.IDiagnosticsUploadRequest, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a DiagnosticsUploadRequest message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns DiagnosticsUploadRequest
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.DiagnosticsUploadRequest;

            /**
             * Gets the default type url for DiagnosticsUploadRequest
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a DiagnosticsSaved. */
        interface IDiagnosticsSaved {

            /** DiagnosticsSaved path */
            path?: (string|null);
        }

        /** Represents a DiagnosticsSaved. */
        class DiagnosticsSaved implements IDiagnosticsSaved {

            /**
             * Constructs a new DiagnosticsSaved.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.IDiagnosticsSaved);

            /** DiagnosticsSaved path. */
            public path: string;

            /**
             * Creates a new DiagnosticsSaved instance using the specified properties.
             * @param [properties] Properties to set
             * @returns DiagnosticsSaved instance
             */
            public static create(properties?: muster.v1.IDiagnosticsSaved): muster.v1.DiagnosticsSaved;

            /**
             * Encodes the specified DiagnosticsSaved message. Does not implicitly {@link muster.v1.DiagnosticsSaved.verify|verify} messages.
             * @param message DiagnosticsSaved message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.IDiagnosticsSaved, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a DiagnosticsSaved message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns DiagnosticsSaved
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.DiagnosticsSaved;

            /**
             * Gets the default type url for DiagnosticsSaved
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a TerminalLeaseRequest. */
        interface ITerminalLeaseRequest {

            /** TerminalLeaseRequest opId */
            opId?: (string|null);

            /** TerminalLeaseRequest session */
            session?: (string|null);
        }

        /** Represents a TerminalLeaseRequest. */
        class TerminalLeaseRequest implements ITerminalLeaseRequest {

            /**
             * Constructs a new TerminalLeaseRequest.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.ITerminalLeaseRequest);

            /** TerminalLeaseRequest opId. */
            public opId: string;

            /** TerminalLeaseRequest session. */
            public session: string;

            /**
             * Creates a new TerminalLeaseRequest instance using the specified properties.
             * @param [properties] Properties to set
             * @returns TerminalLeaseRequest instance
             */
            public static create(properties?: muster.v1.ITerminalLeaseRequest): muster.v1.TerminalLeaseRequest;

            /**
             * Encodes the specified TerminalLeaseRequest message. Does not implicitly {@link muster.v1.TerminalLeaseRequest.verify|verify} messages.
             * @param message TerminalLeaseRequest message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.ITerminalLeaseRequest, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a TerminalLeaseRequest message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns TerminalLeaseRequest
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.TerminalLeaseRequest;

            /**
             * Gets the default type url for TerminalLeaseRequest
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a DeviceLeaseRequest. */
        interface IDeviceLeaseRequest {

            /** DeviceLeaseRequest opId */
            opId?: (string|null);
        }

        /** Represents a DeviceLeaseRequest. */
        class DeviceLeaseRequest implements IDeviceLeaseRequest {

            /**
             * Constructs a new DeviceLeaseRequest.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.IDeviceLeaseRequest);

            /** DeviceLeaseRequest opId. */
            public opId: string;

            /**
             * Creates a new DeviceLeaseRequest instance using the specified properties.
             * @param [properties] Properties to set
             * @returns DeviceLeaseRequest instance
             */
            public static create(properties?: muster.v1.IDeviceLeaseRequest): muster.v1.DeviceLeaseRequest;

            /**
             * Encodes the specified DeviceLeaseRequest message. Does not implicitly {@link muster.v1.DeviceLeaseRequest.verify|verify} messages.
             * @param message DeviceLeaseRequest message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.IDeviceLeaseRequest, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a DeviceLeaseRequest message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns DeviceLeaseRequest
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.DeviceLeaseRequest;

            /**
             * Gets the default type url for DeviceLeaseRequest
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a LockedUsePrepareRequest. */
        interface ILockedUsePrepareRequest {

            /** LockedUsePrepareRequest operation */
            operation?: (muster.v1.ILockedUseOperation|null);
        }

        /** Represents a LockedUsePrepareRequest. */
        class LockedUsePrepareRequest implements ILockedUsePrepareRequest {

            /**
             * Constructs a new LockedUsePrepareRequest.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.ILockedUsePrepareRequest);

            /** LockedUsePrepareRequest operation. */
            public operation?: (muster.v1.ILockedUseOperation|null);

            /**
             * Creates a new LockedUsePrepareRequest instance using the specified properties.
             * @param [properties] Properties to set
             * @returns LockedUsePrepareRequest instance
             */
            public static create(properties?: muster.v1.ILockedUsePrepareRequest): muster.v1.LockedUsePrepareRequest;

            /**
             * Encodes the specified LockedUsePrepareRequest message. Does not implicitly {@link muster.v1.LockedUsePrepareRequest.verify|verify} messages.
             * @param message LockedUsePrepareRequest message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.ILockedUsePrepareRequest, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a LockedUsePrepareRequest message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns LockedUsePrepareRequest
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.LockedUsePrepareRequest;

            /**
             * Gets the default type url for LockedUsePrepareRequest
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a LockedUseChallenge. */
        interface ILockedUseChallenge {

            /** LockedUseChallenge challenge */
            challenge?: (Uint8Array|null);
        }

        /** Represents a LockedUseChallenge. */
        class LockedUseChallenge implements ILockedUseChallenge {

            /**
             * Constructs a new LockedUseChallenge.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.ILockedUseChallenge);

            /** LockedUseChallenge challenge. */
            public challenge: Uint8Array;

            /**
             * Creates a new LockedUseChallenge instance using the specified properties.
             * @param [properties] Properties to set
             * @returns LockedUseChallenge instance
             */
            public static create(properties?: muster.v1.ILockedUseChallenge): muster.v1.LockedUseChallenge;

            /**
             * Encodes the specified LockedUseChallenge message. Does not implicitly {@link muster.v1.LockedUseChallenge.verify|verify} messages.
             * @param message LockedUseChallenge message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.ILockedUseChallenge, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a LockedUseChallenge message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns LockedUseChallenge
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.LockedUseChallenge;

            /**
             * Gets the default type url for LockedUseChallenge
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a LockedUseExecuteRequest. */
        interface ILockedUseExecuteRequest {

            /** LockedUseExecuteRequest challenge */
            challenge?: (Uint8Array|null);

            /** LockedUseExecuteRequest signature */
            signature?: (Uint8Array|null);
        }

        /** Represents a LockedUseExecuteRequest. */
        class LockedUseExecuteRequest implements ILockedUseExecuteRequest {

            /**
             * Constructs a new LockedUseExecuteRequest.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.ILockedUseExecuteRequest);

            /** LockedUseExecuteRequest challenge. */
            public challenge: Uint8Array;

            /** LockedUseExecuteRequest signature. */
            public signature: Uint8Array;

            /**
             * Creates a new LockedUseExecuteRequest instance using the specified properties.
             * @param [properties] Properties to set
             * @returns LockedUseExecuteRequest instance
             */
            public static create(properties?: muster.v1.ILockedUseExecuteRequest): muster.v1.LockedUseExecuteRequest;

            /**
             * Encodes the specified LockedUseExecuteRequest message. Does not implicitly {@link muster.v1.LockedUseExecuteRequest.verify|verify} messages.
             * @param message LockedUseExecuteRequest message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.ILockedUseExecuteRequest, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a LockedUseExecuteRequest message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns LockedUseExecuteRequest
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.LockedUseExecuteRequest;

            /**
             * Gets the default type url for LockedUseExecuteRequest
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a LockedUseRenewRequest. */
        interface ILockedUseRenewRequest {

            /** LockedUseRenewRequest nonce */
            nonce?: (Uint8Array|null);
        }

        /** Represents a LockedUseRenewRequest. */
        class LockedUseRenewRequest implements ILockedUseRenewRequest {

            /**
             * Constructs a new LockedUseRenewRequest.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.ILockedUseRenewRequest);

            /** LockedUseRenewRequest nonce. */
            public nonce: Uint8Array;

            /**
             * Creates a new LockedUseRenewRequest instance using the specified properties.
             * @param [properties] Properties to set
             * @returns LockedUseRenewRequest instance
             */
            public static create(properties?: muster.v1.ILockedUseRenewRequest): muster.v1.LockedUseRenewRequest;

            /**
             * Encodes the specified LockedUseRenewRequest message. Does not implicitly {@link muster.v1.LockedUseRenewRequest.verify|verify} messages.
             * @param message LockedUseRenewRequest message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.ILockedUseRenewRequest, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a LockedUseRenewRequest message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns LockedUseRenewRequest
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.LockedUseRenewRequest;

            /**
             * Gets the default type url for LockedUseRenewRequest
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a LockedUseExtendRequest. */
        interface ILockedUseExtendRequest {

            /** LockedUseExtendRequest challenge */
            challenge?: (Uint8Array|null);

            /** LockedUseExtendRequest signature */
            signature?: (Uint8Array|null);
        }

        /** Represents a LockedUseExtendRequest. */
        class LockedUseExtendRequest implements ILockedUseExtendRequest {

            /**
             * Constructs a new LockedUseExtendRequest.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.ILockedUseExtendRequest);

            /** LockedUseExtendRequest challenge. */
            public challenge: Uint8Array;

            /** LockedUseExtendRequest signature. */
            public signature: Uint8Array;

            /**
             * Creates a new LockedUseExtendRequest instance using the specified properties.
             * @param [properties] Properties to set
             * @returns LockedUseExtendRequest instance
             */
            public static create(properties?: muster.v1.ILockedUseExtendRequest): muster.v1.LockedUseExtendRequest;

            /**
             * Encodes the specified LockedUseExtendRequest message. Does not implicitly {@link muster.v1.LockedUseExtendRequest.verify|verify} messages.
             * @param message LockedUseExtendRequest message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.ILockedUseExtendRequest, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a LockedUseExtendRequest message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns LockedUseExtendRequest
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.LockedUseExtendRequest;

            /**
             * Gets the default type url for LockedUseExtendRequest
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a LockedUseCancelRequest. */
        interface ILockedUseCancelRequest {

            /** LockedUseCancelRequest nonce */
            nonce?: (Uint8Array|null);

            /** LockedUseCancelRequest reason */
            reason?: (string|null);
        }

        /** Represents a LockedUseCancelRequest. */
        class LockedUseCancelRequest implements ILockedUseCancelRequest {

            /**
             * Constructs a new LockedUseCancelRequest.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.ILockedUseCancelRequest);

            /** LockedUseCancelRequest nonce. */
            public nonce: Uint8Array;

            /** LockedUseCancelRequest reason. */
            public reason: string;

            /**
             * Creates a new LockedUseCancelRequest instance using the specified properties.
             * @param [properties] Properties to set
             * @returns LockedUseCancelRequest instance
             */
            public static create(properties?: muster.v1.ILockedUseCancelRequest): muster.v1.LockedUseCancelRequest;

            /**
             * Encodes the specified LockedUseCancelRequest message. Does not implicitly {@link muster.v1.LockedUseCancelRequest.verify|verify} messages.
             * @param message LockedUseCancelRequest message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.ILockedUseCancelRequest, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a LockedUseCancelRequest message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns LockedUseCancelRequest
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.LockedUseCancelRequest;

            /**
             * Gets the default type url for LockedUseCancelRequest
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a NetworkStatusRequest. */
        interface INetworkStatusRequest {
        }

        /** Represents a NetworkStatusRequest. */
        class NetworkStatusRequest implements INetworkStatusRequest {

            /**
             * Constructs a new NetworkStatusRequest.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.INetworkStatusRequest);

            /**
             * Creates a new NetworkStatusRequest instance using the specified properties.
             * @param [properties] Properties to set
             * @returns NetworkStatusRequest instance
             */
            public static create(properties?: muster.v1.INetworkStatusRequest): muster.v1.NetworkStatusRequest;

            /**
             * Encodes the specified NetworkStatusRequest message. Does not implicitly {@link muster.v1.NetworkStatusRequest.verify|verify} messages.
             * @param message NetworkStatusRequest message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.INetworkStatusRequest, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a NetworkStatusRequest message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns NetworkStatusRequest
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.NetworkStatusRequest;

            /**
             * Gets the default type url for NetworkStatusRequest
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a NetworkStatus. */
        interface INetworkStatus {

            /** NetworkStatus hasNetwork */
            hasNetwork?: (boolean|null);

            /** NetworkStatus networkId */
            networkId?: (string|null);

            /** NetworkStatus name */
            name?: (string|null);

            /** NetworkStatus vnetCidr */
            vnetCidr?: (string|null);

            /** NetworkStatus selfId */
            selfId?: (string|null);

            /** NetworkStatus members */
            members?: (muster.v1.IMemberView[]|null);

            /** NetworkStatus relay */
            relay?: (string|null);

            /** NetworkStatus reflectedAddr */
            reflectedAddr?: (string|null);

            /** NetworkStatus pendingJoins */
            pendingJoins?: (number|null);
        }

        /** Represents a NetworkStatus. */
        class NetworkStatus implements INetworkStatus {

            /**
             * Constructs a new NetworkStatus.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.INetworkStatus);

            /** NetworkStatus hasNetwork. */
            public hasNetwork: boolean;

            /** NetworkStatus networkId. */
            public networkId: string;

            /** NetworkStatus name. */
            public name: string;

            /** NetworkStatus vnetCidr. */
            public vnetCidr: string;

            /** NetworkStatus selfId. */
            public selfId: string;

            /** NetworkStatus members. */
            public members: muster.v1.IMemberView[];

            /** NetworkStatus relay. */
            public relay: string;

            /** NetworkStatus reflectedAddr. */
            public reflectedAddr: string;

            /** NetworkStatus pendingJoins. */
            public pendingJoins: number;

            /**
             * Creates a new NetworkStatus instance using the specified properties.
             * @param [properties] Properties to set
             * @returns NetworkStatus instance
             */
            public static create(properties?: muster.v1.INetworkStatus): muster.v1.NetworkStatus;

            /**
             * Encodes the specified NetworkStatus message. Does not implicitly {@link muster.v1.NetworkStatus.verify|verify} messages.
             * @param message NetworkStatus message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.INetworkStatus, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a NetworkStatus message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns NetworkStatus
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.NetworkStatus;

            /**
             * Gets the default type url for NetworkStatus
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a MemberView. */
        interface IMemberView {

            /** MemberView deviceId */
            deviceId?: (string|null);

            /** MemberView fingerprint */
            fingerprint?: (string|null);

            /** MemberView name */
            name?: (string|null);

            /** MemberView kind */
            kind?: (string|null);

            /** MemberView role */
            role?: (string|null);

            /** MemberView vip */
            vip?: (string|null);

            /** MemberView scopes */
            scopes?: (string[]|null);

            /** MemberView isSelf */
            isSelf?: (boolean|null);

            /** MemberView connected */
            connected?: (boolean|null);

            /** MemberView path */
            path?: (string|null);

            /** MemberView rttMs */
            rttMs?: (number|null);

            /** MemberView connectionStatus */
            connectionStatus?: (string|null);

            /** MemberView version */
            version?: (string|null);

            /** MemberView commit */
            commit?: (string|null);

            /** MemberView link */
            link?: (string|null);

            /** MemberView presence */
            presence?: (string|null);
        }

        /** Represents a MemberView. */
        class MemberView implements IMemberView {

            /**
             * Constructs a new MemberView.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.IMemberView);

            /** MemberView deviceId. */
            public deviceId: string;

            /** MemberView fingerprint. */
            public fingerprint: string;

            /** MemberView name. */
            public name: string;

            /** MemberView kind. */
            public kind: string;

            /** MemberView role. */
            public role: string;

            /** MemberView vip. */
            public vip: string;

            /** MemberView scopes. */
            public scopes: string[];

            /** MemberView isSelf. */
            public isSelf: boolean;

            /** MemberView connected. */
            public connected: boolean;

            /** MemberView path. */
            public path: string;

            /** MemberView rttMs. */
            public rttMs: number;

            /** MemberView connectionStatus. */
            public connectionStatus: string;

            /** MemberView version. */
            public version: string;

            /** MemberView commit. */
            public commit: string;

            /** MemberView link. */
            public link: string;

            /** MemberView presence. */
            public presence: string;

            /**
             * Creates a new MemberView instance using the specified properties.
             * @param [properties] Properties to set
             * @returns MemberView instance
             */
            public static create(properties?: muster.v1.IMemberView): muster.v1.MemberView;

            /**
             * Encodes the specified MemberView message. Does not implicitly {@link muster.v1.MemberView.verify|verify} messages.
             * @param message MemberView message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.IMemberView, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a MemberView message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns MemberView
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.MemberView;

            /**
             * Gets the default type url for MemberView
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a NetworkCreateRequest. */
        interface INetworkCreateRequest {

            /** NetworkCreateRequest name */
            name?: (string|null);

            /** NetworkCreateRequest vnetCidr */
            vnetCidr?: (string|null);
        }

        /** Represents a NetworkCreateRequest. */
        class NetworkCreateRequest implements INetworkCreateRequest {

            /**
             * Constructs a new NetworkCreateRequest.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.INetworkCreateRequest);

            /** NetworkCreateRequest name. */
            public name: string;

            /** NetworkCreateRequest vnetCidr. */
            public vnetCidr: string;

            /**
             * Creates a new NetworkCreateRequest instance using the specified properties.
             * @param [properties] Properties to set
             * @returns NetworkCreateRequest instance
             */
            public static create(properties?: muster.v1.INetworkCreateRequest): muster.v1.NetworkCreateRequest;

            /**
             * Encodes the specified NetworkCreateRequest message. Does not implicitly {@link muster.v1.NetworkCreateRequest.verify|verify} messages.
             * @param message NetworkCreateRequest message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.INetworkCreateRequest, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a NetworkCreateRequest message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns NetworkCreateRequest
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.NetworkCreateRequest;

            /**
             * Gets the default type url for NetworkCreateRequest
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a NetworkInviteRequest. */
        interface INetworkInviteRequest {

            /** NetworkInviteRequest ttlSecs */
            ttlSecs?: (number|null);
        }

        /** Represents a NetworkInviteRequest. */
        class NetworkInviteRequest implements INetworkInviteRequest {

            /**
             * Constructs a new NetworkInviteRequest.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.INetworkInviteRequest);

            /** NetworkInviteRequest ttlSecs. */
            public ttlSecs: number;

            /**
             * Creates a new NetworkInviteRequest instance using the specified properties.
             * @param [properties] Properties to set
             * @returns NetworkInviteRequest instance
             */
            public static create(properties?: muster.v1.INetworkInviteRequest): muster.v1.NetworkInviteRequest;

            /**
             * Encodes the specified NetworkInviteRequest message. Does not implicitly {@link muster.v1.NetworkInviteRequest.verify|verify} messages.
             * @param message NetworkInviteRequest message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.INetworkInviteRequest, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a NetworkInviteRequest message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns NetworkInviteRequest
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.NetworkInviteRequest;

            /**
             * Gets the default type url for NetworkInviteRequest
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of an InviteText. */
        interface IInviteText {

            /** InviteText text */
            text?: (string|null);

            /** InviteText expiresUnix */
            expiresUnix?: (number|Long|null);
        }

        /** Represents an InviteText. */
        class InviteText implements IInviteText {

            /**
             * Constructs a new InviteText.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.IInviteText);

            /** InviteText text. */
            public text: string;

            /** InviteText expiresUnix. */
            public expiresUnix: (number|Long);

            /**
             * Creates a new InviteText instance using the specified properties.
             * @param [properties] Properties to set
             * @returns InviteText instance
             */
            public static create(properties?: muster.v1.IInviteText): muster.v1.InviteText;

            /**
             * Encodes the specified InviteText message. Does not implicitly {@link muster.v1.InviteText.verify|verify} messages.
             * @param message InviteText message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.IInviteText, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes an InviteText message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns InviteText
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.InviteText;

            /**
             * Gets the default type url for InviteText
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a NetworkJoinRequest. */
        interface INetworkJoinRequest {

            /** NetworkJoinRequest inviteText */
            inviteText?: (string|null);

            /** NetworkJoinRequest deviceName */
            deviceName?: (string|null);

            /** NetworkJoinRequest deviceKind */
            deviceKind?: (string|null);
        }

        /** Represents a NetworkJoinRequest. */
        class NetworkJoinRequest implements INetworkJoinRequest {

            /**
             * Constructs a new NetworkJoinRequest.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.INetworkJoinRequest);

            /** NetworkJoinRequest inviteText. */
            public inviteText: string;

            /** NetworkJoinRequest deviceName. */
            public deviceName: string;

            /** NetworkJoinRequest deviceKind. */
            public deviceKind: string;

            /**
             * Creates a new NetworkJoinRequest instance using the specified properties.
             * @param [properties] Properties to set
             * @returns NetworkJoinRequest instance
             */
            public static create(properties?: muster.v1.INetworkJoinRequest): muster.v1.NetworkJoinRequest;

            /**
             * Encodes the specified NetworkJoinRequest message. Does not implicitly {@link muster.v1.NetworkJoinRequest.verify|verify} messages.
             * @param message NetworkJoinRequest message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.INetworkJoinRequest, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a NetworkJoinRequest message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns NetworkJoinRequest
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.NetworkJoinRequest;

            /**
             * Gets the default type url for NetworkJoinRequest
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a JoinProgress. */
        interface IJoinProgress {

            /** JoinProgress phase */
            phase?: (muster.v1.JoinProgress.Phase|null);

            /** JoinProgress message */
            message?: (string|null);
        }

        /** Represents a JoinProgress. */
        class JoinProgress implements IJoinProgress {

            /**
             * Constructs a new JoinProgress.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.IJoinProgress);

            /** JoinProgress phase. */
            public phase: muster.v1.JoinProgress.Phase;

            /** JoinProgress message. */
            public message: string;

            /**
             * Creates a new JoinProgress instance using the specified properties.
             * @param [properties] Properties to set
             * @returns JoinProgress instance
             */
            public static create(properties?: muster.v1.IJoinProgress): muster.v1.JoinProgress;

            /**
             * Encodes the specified JoinProgress message. Does not implicitly {@link muster.v1.JoinProgress.verify|verify} messages.
             * @param message JoinProgress message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.IJoinProgress, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a JoinProgress message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns JoinProgress
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.JoinProgress;

            /**
             * Gets the default type url for JoinProgress
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        namespace JoinProgress {

            /** Phase enum. */
            enum Phase {
                CONNECTING = 0,
                WAITING_APPROVAL = 1,
                JOINED = 2,
                FAILED = 3
            }
        }

        /** Properties of a NetworkPendingRequest. */
        interface INetworkPendingRequest {
        }

        /** Represents a NetworkPendingRequest. */
        class NetworkPendingRequest implements INetworkPendingRequest {

            /**
             * Constructs a new NetworkPendingRequest.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.INetworkPendingRequest);

            /**
             * Creates a new NetworkPendingRequest instance using the specified properties.
             * @param [properties] Properties to set
             * @returns NetworkPendingRequest instance
             */
            public static create(properties?: muster.v1.INetworkPendingRequest): muster.v1.NetworkPendingRequest;

            /**
             * Encodes the specified NetworkPendingRequest message. Does not implicitly {@link muster.v1.NetworkPendingRequest.verify|verify} messages.
             * @param message NetworkPendingRequest message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.INetworkPendingRequest, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a NetworkPendingRequest message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns NetworkPendingRequest
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.NetworkPendingRequest;

            /**
             * Gets the default type url for NetworkPendingRequest
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a PendingJoins. */
        interface IPendingJoins {

            /** PendingJoins joins */
            joins?: (muster.v1.IPendingJoin[]|null);
        }

        /** Represents a PendingJoins. */
        class PendingJoins implements IPendingJoins {

            /**
             * Constructs a new PendingJoins.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.IPendingJoins);

            /** PendingJoins joins. */
            public joins: muster.v1.IPendingJoin[];

            /**
             * Creates a new PendingJoins instance using the specified properties.
             * @param [properties] Properties to set
             * @returns PendingJoins instance
             */
            public static create(properties?: muster.v1.IPendingJoins): muster.v1.PendingJoins;

            /**
             * Encodes the specified PendingJoins message. Does not implicitly {@link muster.v1.PendingJoins.verify|verify} messages.
             * @param message PendingJoins message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.IPendingJoins, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a PendingJoins message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns PendingJoins
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.PendingJoins;

            /**
             * Gets the default type url for PendingJoins
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a PendingJoin. */
        interface IPendingJoin {

            /** PendingJoin inviteId */
            inviteId?: (string|null);

            /** PendingJoin deviceId */
            deviceId?: (string|null);

            /** PendingJoin fingerprint */
            fingerprint?: (string|null);

            /** PendingJoin name */
            name?: (string|null);

            /** PendingJoin kind */
            kind?: (string|null);
        }

        /** Represents a PendingJoin. */
        class PendingJoin implements IPendingJoin {

            /**
             * Constructs a new PendingJoin.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.IPendingJoin);

            /** PendingJoin inviteId. */
            public inviteId: string;

            /** PendingJoin deviceId. */
            public deviceId: string;

            /** PendingJoin fingerprint. */
            public fingerprint: string;

            /** PendingJoin name. */
            public name: string;

            /** PendingJoin kind. */
            public kind: string;

            /**
             * Creates a new PendingJoin instance using the specified properties.
             * @param [properties] Properties to set
             * @returns PendingJoin instance
             */
            public static create(properties?: muster.v1.IPendingJoin): muster.v1.PendingJoin;

            /**
             * Encodes the specified PendingJoin message. Does not implicitly {@link muster.v1.PendingJoin.verify|verify} messages.
             * @param message PendingJoin message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.IPendingJoin, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a PendingJoin message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns PendingJoin
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.PendingJoin;

            /**
             * Gets the default type url for PendingJoin
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a NetworkDecideRequest. */
        interface INetworkDecideRequest {

            /** NetworkDecideRequest deviceId */
            deviceId?: (string|null);

            /** NetworkDecideRequest approve */
            approve?: (boolean|null);

            /** NetworkDecideRequest scopes */
            scopes?: (string[]|null);

            /** NetworkDecideRequest reason */
            reason?: (string|null);
        }

        /** Represents a NetworkDecideRequest. */
        class NetworkDecideRequest implements INetworkDecideRequest {

            /**
             * Constructs a new NetworkDecideRequest.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.INetworkDecideRequest);

            /** NetworkDecideRequest deviceId. */
            public deviceId: string;

            /** NetworkDecideRequest approve. */
            public approve: boolean;

            /** NetworkDecideRequest scopes. */
            public scopes: string[];

            /** NetworkDecideRequest reason. */
            public reason: string;

            /**
             * Creates a new NetworkDecideRequest instance using the specified properties.
             * @param [properties] Properties to set
             * @returns NetworkDecideRequest instance
             */
            public static create(properties?: muster.v1.INetworkDecideRequest): muster.v1.NetworkDecideRequest;

            /**
             * Encodes the specified NetworkDecideRequest message. Does not implicitly {@link muster.v1.NetworkDecideRequest.verify|verify} messages.
             * @param message NetworkDecideRequest message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.INetworkDecideRequest, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a NetworkDecideRequest message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns NetworkDecideRequest
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.NetworkDecideRequest;

            /**
             * Gets the default type url for NetworkDecideRequest
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a NetworkRemoveRequest. */
        interface INetworkRemoveRequest {

            /** NetworkRemoveRequest deviceId */
            deviceId?: (string|null);
        }

        /** Represents a NetworkRemoveRequest. */
        class NetworkRemoveRequest implements INetworkRemoveRequest {

            /**
             * Constructs a new NetworkRemoveRequest.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.INetworkRemoveRequest);

            /** NetworkRemoveRequest deviceId. */
            public deviceId: string;

            /**
             * Creates a new NetworkRemoveRequest instance using the specified properties.
             * @param [properties] Properties to set
             * @returns NetworkRemoveRequest instance
             */
            public static create(properties?: muster.v1.INetworkRemoveRequest): muster.v1.NetworkRemoveRequest;

            /**
             * Encodes the specified NetworkRemoveRequest message. Does not implicitly {@link muster.v1.NetworkRemoveRequest.verify|verify} messages.
             * @param message NetworkRemoveRequest message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.INetworkRemoveRequest, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a NetworkRemoveRequest message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns NetworkRemoveRequest
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.NetworkRemoveRequest;

            /**
             * Gets the default type url for NetworkRemoveRequest
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a NetworkSetScopesRequest. */
        interface INetworkSetScopesRequest {

            /** NetworkSetScopesRequest deviceId */
            deviceId?: (string|null);

            /** NetworkSetScopesRequest scopes */
            scopes?: (string[]|null);

            /** NetworkSetScopesRequest role */
            role?: (string|null);
        }

        /** Represents a NetworkSetScopesRequest. */
        class NetworkSetScopesRequest implements INetworkSetScopesRequest {

            /**
             * Constructs a new NetworkSetScopesRequest.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.INetworkSetScopesRequest);

            /** NetworkSetScopesRequest deviceId. */
            public deviceId: string;

            /** NetworkSetScopesRequest scopes. */
            public scopes: string[];

            /** NetworkSetScopesRequest role. */
            public role: string;

            /**
             * Creates a new NetworkSetScopesRequest instance using the specified properties.
             * @param [properties] Properties to set
             * @returns NetworkSetScopesRequest instance
             */
            public static create(properties?: muster.v1.INetworkSetScopesRequest): muster.v1.NetworkSetScopesRequest;

            /**
             * Encodes the specified NetworkSetScopesRequest message. Does not implicitly {@link muster.v1.NetworkSetScopesRequest.verify|verify} messages.
             * @param message NetworkSetScopesRequest message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.INetworkSetScopesRequest, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a NetworkSetScopesRequest message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns NetworkSetScopesRequest
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.NetworkSetScopesRequest;

            /**
             * Gets the default type url for NetworkSetScopesRequest
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a PolicyGetRequest. */
        interface IPolicyGetRequest {
        }

        /** Represents a PolicyGetRequest. */
        class PolicyGetRequest implements IPolicyGetRequest {

            /**
             * Constructs a new PolicyGetRequest.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.IPolicyGetRequest);

            /**
             * Creates a new PolicyGetRequest instance using the specified properties.
             * @param [properties] Properties to set
             * @returns PolicyGetRequest instance
             */
            public static create(properties?: muster.v1.IPolicyGetRequest): muster.v1.PolicyGetRequest;

            /**
             * Encodes the specified PolicyGetRequest message. Does not implicitly {@link muster.v1.PolicyGetRequest.verify|verify} messages.
             * @param message PolicyGetRequest message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.IPolicyGetRequest, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a PolicyGetRequest message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns PolicyGetRequest
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.PolicyGetRequest;

            /**
             * Gets the default type url for PolicyGetRequest
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a Policy. */
        interface IPolicy {

            /** Policy openPorts */
            openPorts?: (string[]|null);

            /** Policy allowLockedControl */
            allowLockedControl?: (boolean|null);

            /** Policy axAllowlist */
            axAllowlist?: (string[]|null);

            /** Policy offerAgent */
            offerAgent?: (boolean|null);

            /** Policy offerTerminal */
            offerTerminal?: (boolean|null);

            /** Policy offerTasks */
            offerTasks?: (boolean|null);

            /** Policy webWindow */
            webWindow?: (muster.v1.IWebWindow|null);
        }

        /** Represents a Policy. */
        class Policy implements IPolicy {

            /**
             * Constructs a new Policy.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.IPolicy);

            /** Policy openPorts. */
            public openPorts: string[];

            /** Policy allowLockedControl. */
            public allowLockedControl: boolean;

            /** Policy axAllowlist. */
            public axAllowlist: string[];

            /** Policy offerAgent. */
            public offerAgent: boolean;

            /** Policy offerTerminal. */
            public offerTerminal: boolean;

            /** Policy offerTasks. */
            public offerTasks: boolean;

            /** Policy webWindow. */
            public webWindow?: (muster.v1.IWebWindow|null);

            /**
             * Creates a new Policy instance using the specified properties.
             * @param [properties] Properties to set
             * @returns Policy instance
             */
            public static create(properties?: muster.v1.IPolicy): muster.v1.Policy;

            /**
             * Encodes the specified Policy message. Does not implicitly {@link muster.v1.Policy.verify|verify} messages.
             * @param message Policy message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.IPolicy, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a Policy message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns Policy
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.Policy;

            /**
             * Gets the default type url for Policy
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a WebWindow. */
        interface IWebWindow {

            /** WebWindow origin */
            origin?: (string|null);
        }

        /** Represents a WebWindow. */
        class WebWindow implements IWebWindow {

            /**
             * Constructs a new WebWindow.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.IWebWindow);

            /** WebWindow origin. */
            public origin: string;

            /**
             * Creates a new WebWindow instance using the specified properties.
             * @param [properties] Properties to set
             * @returns WebWindow instance
             */
            public static create(properties?: muster.v1.IWebWindow): muster.v1.WebWindow;

            /**
             * Encodes the specified WebWindow message. Does not implicitly {@link muster.v1.WebWindow.verify|verify} messages.
             * @param message WebWindow message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.IWebWindow, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a WebWindow message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns WebWindow
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.WebWindow;

            /**
             * Gets the default type url for WebWindow
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a PolicySetRequest. */
        interface IPolicySetRequest {

            /** PolicySetRequest policy */
            policy?: (muster.v1.IPolicy|null);
        }

        /** Represents a PolicySetRequest. */
        class PolicySetRequest implements IPolicySetRequest {

            /**
             * Constructs a new PolicySetRequest.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.IPolicySetRequest);

            /** PolicySetRequest policy. */
            public policy?: (muster.v1.IPolicy|null);

            /**
             * Creates a new PolicySetRequest instance using the specified properties.
             * @param [properties] Properties to set
             * @returns PolicySetRequest instance
             */
            public static create(properties?: muster.v1.IPolicySetRequest): muster.v1.PolicySetRequest;

            /**
             * Encodes the specified PolicySetRequest message. Does not implicitly {@link muster.v1.PolicySetRequest.verify|verify} messages.
             * @param message PolicySetRequest message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.IPolicySetRequest, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a PolicySetRequest message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns PolicySetRequest
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.PolicySetRequest;

            /**
             * Gets the default type url for PolicySetRequest
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a ForwardOpenRequest. */
        interface IForwardOpenRequest {

            /** ForwardOpenRequest device */
            device?: (string|null);

            /** ForwardOpenRequest port */
            port?: (number|null);

            /** ForwardOpenRequest localPort */
            localPort?: (number|null);

            /** ForwardOpenRequest window */
            window?: (string|null);
        }

        /** Represents a ForwardOpenRequest. */
        class ForwardOpenRequest implements IForwardOpenRequest {

            /**
             * Constructs a new ForwardOpenRequest.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.IForwardOpenRequest);

            /** ForwardOpenRequest device. */
            public device: string;

            /** ForwardOpenRequest port. */
            public port: number;

            /** ForwardOpenRequest localPort. */
            public localPort: number;

            /** ForwardOpenRequest window. */
            public window: string;

            /**
             * Creates a new ForwardOpenRequest instance using the specified properties.
             * @param [properties] Properties to set
             * @returns ForwardOpenRequest instance
             */
            public static create(properties?: muster.v1.IForwardOpenRequest): muster.v1.ForwardOpenRequest;

            /**
             * Encodes the specified ForwardOpenRequest message. Does not implicitly {@link muster.v1.ForwardOpenRequest.verify|verify} messages.
             * @param message ForwardOpenRequest message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.IForwardOpenRequest, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a ForwardOpenRequest message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns ForwardOpenRequest
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.ForwardOpenRequest;

            /**
             * Gets the default type url for ForwardOpenRequest
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a ForwardOpened. */
        interface IForwardOpened {

            /** ForwardOpened localPort */
            localPort?: (number|null);
        }

        /** Represents a ForwardOpened. */
        class ForwardOpened implements IForwardOpened {

            /**
             * Constructs a new ForwardOpened.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.IForwardOpened);

            /** ForwardOpened localPort. */
            public localPort: number;

            /**
             * Creates a new ForwardOpened instance using the specified properties.
             * @param [properties] Properties to set
             * @returns ForwardOpened instance
             */
            public static create(properties?: muster.v1.IForwardOpened): muster.v1.ForwardOpened;

            /**
             * Encodes the specified ForwardOpened message. Does not implicitly {@link muster.v1.ForwardOpened.verify|verify} messages.
             * @param message ForwardOpened message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.IForwardOpened, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a ForwardOpened message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns ForwardOpened
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.ForwardOpened;

            /**
             * Gets the default type url for ForwardOpened
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a TerminalSessionsRequest. */
        interface ITerminalSessionsRequest {

            /** TerminalSessionsRequest watch */
            watch?: (boolean|null);
        }

        /** Represents a TerminalSessionsRequest. */
        class TerminalSessionsRequest implements ITerminalSessionsRequest {

            /**
             * Constructs a new TerminalSessionsRequest.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.ITerminalSessionsRequest);

            /** TerminalSessionsRequest watch. */
            public watch: boolean;

            /**
             * Creates a new TerminalSessionsRequest instance using the specified properties.
             * @param [properties] Properties to set
             * @returns TerminalSessionsRequest instance
             */
            public static create(properties?: muster.v1.ITerminalSessionsRequest): muster.v1.TerminalSessionsRequest;

            /**
             * Encodes the specified TerminalSessionsRequest message. Does not implicitly {@link muster.v1.TerminalSessionsRequest.verify|verify} messages.
             * @param message TerminalSessionsRequest message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.ITerminalSessionsRequest, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a TerminalSessionsRequest message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns TerminalSessionsRequest
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.TerminalSessionsRequest;

            /**
             * Gets the default type url for TerminalSessionsRequest
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a TerminalSession. */
        interface ITerminalSession {

            /** TerminalSession id */
            id?: (string|null);

            /** TerminalSession title */
            title?: (string|null);

            /** TerminalSession cols */
            cols?: (number|null);

            /** TerminalSession rows */
            rows?: (number|null);

            /** TerminalSession windowId */
            windowId?: (string|null);

            /** TerminalSession tabId */
            tabId?: (string|null);

            /** TerminalSession source */
            source?: (string|null);

            /** TerminalSession deviceId */
            deviceId?: (string|null);

            /** TerminalSession deviceName */
            deviceName?: (string|null);
        }

        /** Represents a TerminalSession. */
        class TerminalSession implements ITerminalSession {

            /**
             * Constructs a new TerminalSession.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.ITerminalSession);

            /** TerminalSession id. */
            public id: string;

            /** TerminalSession title. */
            public title: string;

            /** TerminalSession cols. */
            public cols: number;

            /** TerminalSession rows. */
            public rows: number;

            /** TerminalSession windowId. */
            public windowId: string;

            /** TerminalSession tabId. */
            public tabId: string;

            /** TerminalSession source. */
            public source: string;

            /** TerminalSession deviceId. */
            public deviceId: string;

            /** TerminalSession deviceName. */
            public deviceName: string;

            /**
             * Creates a new TerminalSession instance using the specified properties.
             * @param [properties] Properties to set
             * @returns TerminalSession instance
             */
            public static create(properties?: muster.v1.ITerminalSession): muster.v1.TerminalSession;

            /**
             * Encodes the specified TerminalSession message. Does not implicitly {@link muster.v1.TerminalSession.verify|verify} messages.
             * @param message TerminalSession message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.ITerminalSession, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a TerminalSession message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns TerminalSession
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.TerminalSession;

            /**
             * Gets the default type url for TerminalSession
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a TerminalSource. */
        interface ITerminalSource {

            /** TerminalSource source */
            source?: (string|null);

            /** TerminalSource label */
            label?: (string|null);
        }

        /** Represents a TerminalSource. */
        class TerminalSource implements ITerminalSource {

            /**
             * Constructs a new TerminalSource.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.ITerminalSource);

            /** TerminalSource source. */
            public source: string;

            /** TerminalSource label. */
            public label: string;

            /**
             * Creates a new TerminalSource instance using the specified properties.
             * @param [properties] Properties to set
             * @returns TerminalSource instance
             */
            public static create(properties?: muster.v1.ITerminalSource): muster.v1.TerminalSource;

            /**
             * Encodes the specified TerminalSource message. Does not implicitly {@link muster.v1.TerminalSource.verify|verify} messages.
             * @param message TerminalSource message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.ITerminalSource, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a TerminalSource message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns TerminalSource
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.TerminalSource;

            /**
             * Gets the default type url for TerminalSource
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a TerminalSessionList. */
        interface ITerminalSessionList {

            /** TerminalSessionList sessions */
            sessions?: (muster.v1.ITerminalSession[]|null);

            /** TerminalSessionList sources */
            sources?: (muster.v1.ITerminalSource[]|null);

            /** TerminalSessionList errors */
            errors?: (string[]|null);
        }

        /** Represents a TerminalSessionList. */
        class TerminalSessionList implements ITerminalSessionList {

            /**
             * Constructs a new TerminalSessionList.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.ITerminalSessionList);

            /** TerminalSessionList sessions. */
            public sessions: muster.v1.ITerminalSession[];

            /** TerminalSessionList sources. */
            public sources: muster.v1.ITerminalSource[];

            /** TerminalSessionList errors. */
            public errors: string[];

            /**
             * Creates a new TerminalSessionList instance using the specified properties.
             * @param [properties] Properties to set
             * @returns TerminalSessionList instance
             */
            public static create(properties?: muster.v1.ITerminalSessionList): muster.v1.TerminalSessionList;

            /**
             * Encodes the specified TerminalSessionList message. Does not implicitly {@link muster.v1.TerminalSessionList.verify|verify} messages.
             * @param message TerminalSessionList message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.ITerminalSessionList, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a TerminalSessionList message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns TerminalSessionList
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.TerminalSessionList;

            /**
             * Gets the default type url for TerminalSessionList
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a TerminalAttachRequest. */
        interface ITerminalAttachRequest {

            /** TerminalAttachRequest session */
            session?: (string|null);

            /** TerminalAttachRequest sinceRev */
            sinceRev?: (number|Long|null);
        }

        /** Represents a TerminalAttachRequest. */
        class TerminalAttachRequest implements ITerminalAttachRequest {

            /**
             * Constructs a new TerminalAttachRequest.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.ITerminalAttachRequest);

            /** TerminalAttachRequest session. */
            public session: string;

            /** TerminalAttachRequest sinceRev. */
            public sinceRev: (number|Long);

            /**
             * Creates a new TerminalAttachRequest instance using the specified properties.
             * @param [properties] Properties to set
             * @returns TerminalAttachRequest instance
             */
            public static create(properties?: muster.v1.ITerminalAttachRequest): muster.v1.TerminalAttachRequest;

            /**
             * Encodes the specified TerminalAttachRequest message. Does not implicitly {@link muster.v1.TerminalAttachRequest.verify|verify} messages.
             * @param message TerminalAttachRequest message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.ITerminalAttachRequest, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a TerminalAttachRequest message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns TerminalAttachRequest
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.TerminalAttachRequest;

            /**
             * Gets the default type url for TerminalAttachRequest
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a TerminalLine. */
        interface ITerminalLine {

            /** TerminalLine index */
            index?: (number|null);

            /** TerminalLine text */
            text?: (string|null);

            /** TerminalLine styles */
            styles?: (Uint8Array|null);
        }

        /** Represents a TerminalLine. */
        class TerminalLine implements ITerminalLine {

            /**
             * Constructs a new TerminalLine.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.ITerminalLine);

            /** TerminalLine index. */
            public index: number;

            /** TerminalLine text. */
            public text: string;

            /** TerminalLine styles. */
            public styles: Uint8Array;

            /**
             * Creates a new TerminalLine instance using the specified properties.
             * @param [properties] Properties to set
             * @returns TerminalLine instance
             */
            public static create(properties?: muster.v1.ITerminalLine): muster.v1.TerminalLine;

            /**
             * Encodes the specified TerminalLine message. Does not implicitly {@link muster.v1.TerminalLine.verify|verify} messages.
             * @param message TerminalLine message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.ITerminalLine, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a TerminalLine message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns TerminalLine
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.TerminalLine;

            /**
             * Gets the default type url for TerminalLine
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a TerminalFrame. */
        interface ITerminalFrame {

            /** TerminalFrame session */
            session?: (string|null);

            /** TerminalFrame rev */
            rev?: (number|Long|null);

            /** TerminalFrame full */
            full?: (boolean|null);

            /** TerminalFrame cols */
            cols?: (number|null);

            /** TerminalFrame rows */
            rows?: (number|null);

            /** TerminalFrame lines */
            lines?: (muster.v1.ITerminalLine[]|null);

            /** TerminalFrame cursorX */
            cursorX?: (number|null);

            /** TerminalFrame cursorY */
            cursorY?: (number|null);

            /** TerminalFrame ended */
            ended?: (boolean|null);

            /** TerminalFrame desktopAttachCommand */
            desktopAttachCommand?: (string|null);

            /** TerminalFrame iterm2AttachCommand */
            iterm2AttachCommand?: (string|null);

            /** TerminalFrame nativeSources */
            nativeSources?: (muster.v1.ITerminalSource[]|null);
        }

        /** Represents a TerminalFrame. */
        class TerminalFrame implements ITerminalFrame {

            /**
             * Constructs a new TerminalFrame.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.ITerminalFrame);

            /** TerminalFrame session. */
            public session: string;

            /** TerminalFrame rev. */
            public rev: (number|Long);

            /** TerminalFrame full. */
            public full: boolean;

            /** TerminalFrame cols. */
            public cols: number;

            /** TerminalFrame rows. */
            public rows: number;

            /** TerminalFrame lines. */
            public lines: muster.v1.ITerminalLine[];

            /** TerminalFrame cursorX. */
            public cursorX: number;

            /** TerminalFrame cursorY. */
            public cursorY: number;

            /** TerminalFrame ended. */
            public ended: boolean;

            /** TerminalFrame desktopAttachCommand. */
            public desktopAttachCommand: string;

            /** TerminalFrame iterm2AttachCommand. */
            public iterm2AttachCommand: string;

            /** TerminalFrame nativeSources. */
            public nativeSources: muster.v1.ITerminalSource[];

            /**
             * Creates a new TerminalFrame instance using the specified properties.
             * @param [properties] Properties to set
             * @returns TerminalFrame instance
             */
            public static create(properties?: muster.v1.ITerminalFrame): muster.v1.TerminalFrame;

            /**
             * Encodes the specified TerminalFrame message. Does not implicitly {@link muster.v1.TerminalFrame.verify|verify} messages.
             * @param message TerminalFrame message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.ITerminalFrame, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a TerminalFrame message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns TerminalFrame
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.TerminalFrame;

            /**
             * Gets the default type url for TerminalFrame
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a TerminalInputRequest. */
        interface ITerminalInputRequest {

            /** TerminalInputRequest opId */
            opId?: (string|null);

            /** TerminalInputRequest session */
            session?: (string|null);

            /** TerminalInputRequest bytes */
            bytes?: (Uint8Array|null);
        }

        /** Represents a TerminalInputRequest. */
        class TerminalInputRequest implements ITerminalInputRequest {

            /**
             * Constructs a new TerminalInputRequest.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.ITerminalInputRequest);

            /** TerminalInputRequest opId. */
            public opId: string;

            /** TerminalInputRequest session. */
            public session: string;

            /** TerminalInputRequest bytes. */
            public bytes: Uint8Array;

            /**
             * Creates a new TerminalInputRequest instance using the specified properties.
             * @param [properties] Properties to set
             * @returns TerminalInputRequest instance
             */
            public static create(properties?: muster.v1.ITerminalInputRequest): muster.v1.TerminalInputRequest;

            /**
             * Encodes the specified TerminalInputRequest message. Does not implicitly {@link muster.v1.TerminalInputRequest.verify|verify} messages.
             * @param message TerminalInputRequest message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.ITerminalInputRequest, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a TerminalInputRequest message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns TerminalInputRequest
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.TerminalInputRequest;

            /**
             * Gets the default type url for TerminalInputRequest
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a TerminalCreateRequest. */
        interface ITerminalCreateRequest {

            /** TerminalCreateRequest profile */
            profile?: (string|null);

            /** TerminalCreateRequest background */
            background?: (boolean|null);

            /** TerminalCreateRequest source */
            source?: (string|null);

            /** TerminalCreateRequest managedSession */
            managedSession?: (string|null);

            /** TerminalCreateRequest opId */
            opId?: (string|null);
        }

        /** Represents a TerminalCreateRequest. */
        class TerminalCreateRequest implements ITerminalCreateRequest {

            /**
             * Constructs a new TerminalCreateRequest.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.ITerminalCreateRequest);

            /** TerminalCreateRequest profile. */
            public profile: string;

            /** TerminalCreateRequest background. */
            public background: boolean;

            /** TerminalCreateRequest source. */
            public source: string;

            /** TerminalCreateRequest managedSession. */
            public managedSession: string;

            /** TerminalCreateRequest opId. */
            public opId: string;

            /**
             * Creates a new TerminalCreateRequest instance using the specified properties.
             * @param [properties] Properties to set
             * @returns TerminalCreateRequest instance
             */
            public static create(properties?: muster.v1.ITerminalCreateRequest): muster.v1.TerminalCreateRequest;

            /**
             * Encodes the specified TerminalCreateRequest message. Does not implicitly {@link muster.v1.TerminalCreateRequest.verify|verify} messages.
             * @param message TerminalCreateRequest message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.ITerminalCreateRequest, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a TerminalCreateRequest message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns TerminalCreateRequest
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.TerminalCreateRequest;

            /**
             * Gets the default type url for TerminalCreateRequest
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a TerminalResizeRequest. */
        interface ITerminalResizeRequest {

            /** TerminalResizeRequest session */
            session?: (string|null);

            /** TerminalResizeRequest cols */
            cols?: (number|null);

            /** TerminalResizeRequest rows */
            rows?: (number|null);
        }

        /** Represents a TerminalResizeRequest. */
        class TerminalResizeRequest implements ITerminalResizeRequest {

            /**
             * Constructs a new TerminalResizeRequest.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.ITerminalResizeRequest);

            /** TerminalResizeRequest session. */
            public session: string;

            /** TerminalResizeRequest cols. */
            public cols: number;

            /** TerminalResizeRequest rows. */
            public rows: number;

            /**
             * Creates a new TerminalResizeRequest instance using the specified properties.
             * @param [properties] Properties to set
             * @returns TerminalResizeRequest instance
             */
            public static create(properties?: muster.v1.ITerminalResizeRequest): muster.v1.TerminalResizeRequest;

            /**
             * Encodes the specified TerminalResizeRequest message. Does not implicitly {@link muster.v1.TerminalResizeRequest.verify|verify} messages.
             * @param message TerminalResizeRequest message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.ITerminalResizeRequest, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a TerminalResizeRequest message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns TerminalResizeRequest
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.TerminalResizeRequest;

            /**
             * Gets the default type url for TerminalResizeRequest
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a TerminalCloseRequest. */
        interface ITerminalCloseRequest {

            /** TerminalCloseRequest session */
            session?: (string|null);
        }

        /** Represents a TerminalCloseRequest. */
        class TerminalCloseRequest implements ITerminalCloseRequest {

            /**
             * Constructs a new TerminalCloseRequest.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.ITerminalCloseRequest);

            /** TerminalCloseRequest session. */
            public session: string;

            /**
             * Creates a new TerminalCloseRequest instance using the specified properties.
             * @param [properties] Properties to set
             * @returns TerminalCloseRequest instance
             */
            public static create(properties?: muster.v1.ITerminalCloseRequest): muster.v1.TerminalCloseRequest;

            /**
             * Encodes the specified TerminalCloseRequest message. Does not implicitly {@link muster.v1.TerminalCloseRequest.verify|verify} messages.
             * @param message TerminalCloseRequest message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.ITerminalCloseRequest, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a TerminalCloseRequest message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns TerminalCloseRequest
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.TerminalCloseRequest;

            /**
             * Gets the default type url for TerminalCloseRequest
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a TerminalHistoryRequest. */
        interface ITerminalHistoryRequest {

            /** TerminalHistoryRequest session */
            session?: (string|null);

            /** TerminalHistoryRequest lines */
            lines?: (number|null);
        }

        /** Represents a TerminalHistoryRequest. */
        class TerminalHistoryRequest implements ITerminalHistoryRequest {

            /**
             * Constructs a new TerminalHistoryRequest.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.ITerminalHistoryRequest);

            /** TerminalHistoryRequest session. */
            public session: string;

            /** TerminalHistoryRequest lines. */
            public lines: number;

            /**
             * Creates a new TerminalHistoryRequest instance using the specified properties.
             * @param [properties] Properties to set
             * @returns TerminalHistoryRequest instance
             */
            public static create(properties?: muster.v1.ITerminalHistoryRequest): muster.v1.TerminalHistoryRequest;

            /**
             * Encodes the specified TerminalHistoryRequest message. Does not implicitly {@link muster.v1.TerminalHistoryRequest.verify|verify} messages.
             * @param message TerminalHistoryRequest message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.ITerminalHistoryRequest, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a TerminalHistoryRequest message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns TerminalHistoryRequest
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.TerminalHistoryRequest;

            /**
             * Gets the default type url for TerminalHistoryRequest
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a TerminalLines. */
        interface ITerminalLines {

            /** TerminalLines lines */
            lines?: (muster.v1.ITerminalLine[]|null);
        }

        /** Represents a TerminalLines. */
        class TerminalLines implements ITerminalLines {

            /**
             * Constructs a new TerminalLines.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.ITerminalLines);

            /** TerminalLines lines. */
            public lines: muster.v1.ITerminalLine[];

            /**
             * Creates a new TerminalLines instance using the specified properties.
             * @param [properties] Properties to set
             * @returns TerminalLines instance
             */
            public static create(properties?: muster.v1.ITerminalLines): muster.v1.TerminalLines;

            /**
             * Encodes the specified TerminalLines message. Does not implicitly {@link muster.v1.TerminalLines.verify|verify} messages.
             * @param message TerminalLines message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.ITerminalLines, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a TerminalLines message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns TerminalLines
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.TerminalLines;

            /**
             * Gets the default type url for TerminalLines
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of an AgentSession. */
        interface IAgentSession {

            /** AgentSession id */
            id?: (string|null);

            /** AgentSession provider */
            provider?: (string|null);

            /** AgentSession title */
            title?: (string|null);

            /** AgentSession cwd */
            cwd?: (string|null);

            /** AgentSession state */
            state?: (string|null);

            /** AgentSession updatedUnix */
            updatedUnix?: (number|Long|null);

            /** AgentSession messageCount */
            messageCount?: (number|null);

            /** AgentSession controller */
            controller?: (string|null);

            /** AgentSession archived */
            archived?: (boolean|null);

            /** AgentSession deviceId */
            deviceId?: (string|null);

            /** AgentSession deviceName */
            deviceName?: (string|null);

            /** AgentSession processState */
            processState?: (string|null);

            /** AgentSession mode */
            mode?: (string|null);

            /** AgentSession parentSessionId */
            parentSessionId?: (string|null);

            /** AgentSession subagents */
            subagents?: (muster.v1.IAgentSession[]|null);

            /** AgentSession readOnly */
            readOnly?: (boolean|null);

            /** AgentSession transcriptNotice */
            transcriptNotice?: (string|null);

            /** AgentSession composerSuggestion */
            composerSuggestion?: (string|null);

            /** AgentSession descendantActivity */
            descendantActivity?: (string|null);

            /** AgentSession relationship */
            relationship?: (string|null);

            /** AgentSession subsessions */
            subsessions?: (muster.v1.IAgentSession[]|null);
        }

        /** Represents an AgentSession. */
        class AgentSession implements IAgentSession {

            /**
             * Constructs a new AgentSession.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.IAgentSession);

            /** AgentSession id. */
            public id: string;

            /** AgentSession provider. */
            public provider: string;

            /** AgentSession title. */
            public title: string;

            /** AgentSession cwd. */
            public cwd: string;

            /** AgentSession state. */
            public state: string;

            /** AgentSession updatedUnix. */
            public updatedUnix: (number|Long);

            /** AgentSession messageCount. */
            public messageCount: number;

            /** AgentSession controller. */
            public controller: string;

            /** AgentSession archived. */
            public archived: boolean;

            /** AgentSession deviceId. */
            public deviceId: string;

            /** AgentSession deviceName. */
            public deviceName: string;

            /** AgentSession processState. */
            public processState: string;

            /** AgentSession mode. */
            public mode: string;

            /** AgentSession parentSessionId. */
            public parentSessionId: string;

            /** AgentSession subagents. */
            public subagents: muster.v1.IAgentSession[];

            /** AgentSession readOnly. */
            public readOnly: boolean;

            /** AgentSession transcriptNotice. */
            public transcriptNotice: string;

            /** AgentSession composerSuggestion. */
            public composerSuggestion: string;

            /** AgentSession descendantActivity. */
            public descendantActivity: string;

            /** AgentSession relationship. */
            public relationship: string;

            /** AgentSession subsessions. */
            public subsessions: muster.v1.IAgentSession[];

            /**
             * Creates a new AgentSession instance using the specified properties.
             * @param [properties] Properties to set
             * @returns AgentSession instance
             */
            public static create(properties?: muster.v1.IAgentSession): muster.v1.AgentSession;

            /**
             * Encodes the specified AgentSession message. Does not implicitly {@link muster.v1.AgentSession.verify|verify} messages.
             * @param message AgentSession message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.IAgentSession, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes an AgentSession message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns AgentSession
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.AgentSession;

            /**
             * Gets the default type url for AgentSession
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of an AgentListRequest. */
        interface IAgentListRequest {

            /** AgentListRequest limit */
            limit?: (number|null);

            /** AgentListRequest provider */
            provider?: (string|null);
        }

        /** Represents an AgentListRequest. */
        class AgentListRequest implements IAgentListRequest {

            /**
             * Constructs a new AgentListRequest.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.IAgentListRequest);

            /** AgentListRequest limit. */
            public limit: number;

            /** AgentListRequest provider. */
            public provider: string;

            /**
             * Creates a new AgentListRequest instance using the specified properties.
             * @param [properties] Properties to set
             * @returns AgentListRequest instance
             */
            public static create(properties?: muster.v1.IAgentListRequest): muster.v1.AgentListRequest;

            /**
             * Encodes the specified AgentListRequest message. Does not implicitly {@link muster.v1.AgentListRequest.verify|verify} messages.
             * @param message AgentListRequest message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.IAgentListRequest, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes an AgentListRequest message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns AgentListRequest
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.AgentListRequest;

            /**
             * Gets the default type url for AgentListRequest
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of an AgentSessionPage. */
        interface IAgentSessionPage {

            /** AgentSessionPage sessions */
            sessions?: (muster.v1.IAgentSession[]|null);
        }

        /** Represents an AgentSessionPage. */
        class AgentSessionPage implements IAgentSessionPage {

            /**
             * Constructs a new AgentSessionPage.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.IAgentSessionPage);

            /** AgentSessionPage sessions. */
            public sessions: muster.v1.IAgentSession[];

            /**
             * Creates a new AgentSessionPage instance using the specified properties.
             * @param [properties] Properties to set
             * @returns AgentSessionPage instance
             */
            public static create(properties?: muster.v1.IAgentSessionPage): muster.v1.AgentSessionPage;

            /**
             * Encodes the specified AgentSessionPage message. Does not implicitly {@link muster.v1.AgentSessionPage.verify|verify} messages.
             * @param message AgentSessionPage message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.IAgentSessionPage, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes an AgentSessionPage message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns AgentSessionPage
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.AgentSessionPage;

            /**
             * Gets the default type url for AgentSessionPage
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of an AgentWatchRequest. */
        interface IAgentWatchRequest {

            /** AgentWatchRequest session */
            session?: (string|null);

            /** AgentWatchRequest catalogOnly */
            catalogOnly?: (boolean|null);

            /** AgentWatchRequest limit */
            limit?: (number|null);
        }

        /** Represents an AgentWatchRequest. */
        class AgentWatchRequest implements IAgentWatchRequest {

            /**
             * Constructs a new AgentWatchRequest.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.IAgentWatchRequest);

            /** AgentWatchRequest session. */
            public session: string;

            /** AgentWatchRequest catalogOnly. */
            public catalogOnly: boolean;

            /** AgentWatchRequest limit. */
            public limit: number;

            /**
             * Creates a new AgentWatchRequest instance using the specified properties.
             * @param [properties] Properties to set
             * @returns AgentWatchRequest instance
             */
            public static create(properties?: muster.v1.IAgentWatchRequest): muster.v1.AgentWatchRequest;

            /**
             * Encodes the specified AgentWatchRequest message. Does not implicitly {@link muster.v1.AgentWatchRequest.verify|verify} messages.
             * @param message AgentWatchRequest message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.IAgentWatchRequest, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes an AgentWatchRequest message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns AgentWatchRequest
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.AgentWatchRequest;

            /**
             * Gets the default type url for AgentWatchRequest
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of an AgentMessage. */
        interface IAgentMessage {

            /** AgentMessage id */
            id?: (string|null);

            /** AgentMessage role */
            role?: (string|null);

            /** AgentMessage text */
            text?: (string|null);

            /** AgentMessage tsUnix */
            tsUnix?: (number|Long|null);

            /** AgentMessage kind */
            kind?: (string|null);

            /** AgentMessage usage */
            usage?: (muster.v1.ITurnUsage|null);

            /** AgentMessage phase */
            phase?: (string|null);

            /** AgentMessage toolName */
            toolName?: (string|null);

            /** AgentMessage toolCallId */
            toolCallId?: (string|null);

            /** AgentMessage toolStatus */
            toolStatus?: (string|null);
        }

        /** Represents an AgentMessage. */
        class AgentMessage implements IAgentMessage {

            /**
             * Constructs a new AgentMessage.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.IAgentMessage);

            /** AgentMessage id. */
            public id: string;

            /** AgentMessage role. */
            public role: string;

            /** AgentMessage text. */
            public text: string;

            /** AgentMessage tsUnix. */
            public tsUnix: (number|Long);

            /** AgentMessage kind. */
            public kind: string;

            /** AgentMessage usage. */
            public usage?: (muster.v1.ITurnUsage|null);

            /** AgentMessage phase. */
            public phase: string;

            /** AgentMessage toolName. */
            public toolName: string;

            /** AgentMessage toolCallId. */
            public toolCallId: string;

            /** AgentMessage toolStatus. */
            public toolStatus: string;

            /**
             * Creates a new AgentMessage instance using the specified properties.
             * @param [properties] Properties to set
             * @returns AgentMessage instance
             */
            public static create(properties?: muster.v1.IAgentMessage): muster.v1.AgentMessage;

            /**
             * Encodes the specified AgentMessage message. Does not implicitly {@link muster.v1.AgentMessage.verify|verify} messages.
             * @param message AgentMessage message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.IAgentMessage, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes an AgentMessage message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns AgentMessage
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.AgentMessage;

            /**
             * Gets the default type url for AgentMessage
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a TurnUsage. */
        interface ITurnUsage {

            /** TurnUsage model */
            model?: (string|null);

            /** TurnUsage inputTokens */
            inputTokens?: (number|Long|null);

            /** TurnUsage outputTokens */
            outputTokens?: (number|Long|null);

            /** TurnUsage cacheReadTokens */
            cacheReadTokens?: (number|Long|null);

            /** TurnUsage cacheWriteTokens */
            cacheWriteTokens?: (number|Long|null);

            /** TurnUsage tokensKnown */
            tokensKnown?: (boolean|null);

            /** TurnUsage cacheHitPercent */
            cacheHitPercent?: (number|null);

            /** TurnUsage costKnown */
            costKnown?: (boolean|null);

            /** TurnUsage costUsd */
            costUsd?: (number|null);

            /** TurnUsage costSource */
            costSource?: (string|null);

            /** TurnUsage pricingDate */
            pricingDate?: (string|null);

            /** TurnUsage changes */
            changes?: (muster.v1.ITurnFileChange[]|null);

            /** TurnUsage changesIncomplete */
            changesIncomplete?: (boolean|null);

            /** TurnUsage cacheHitKnown */
            cacheHitKnown?: (boolean|null);

            /** TurnUsage requestCount */
            requestCount?: (number|Long|null);

            /** TurnUsage firstRequestCacheHitKnown */
            firstRequestCacheHitKnown?: (boolean|null);

            /** TurnUsage firstRequestCacheHitPercent */
            firstRequestCacheHitPercent?: (number|null);

            /** TurnUsage cacheExpiry */
            cacheExpiry?: (muster.v1.ICacheExpiry|null);
        }

        /** Represents a TurnUsage. */
        class TurnUsage implements ITurnUsage {

            /**
             * Constructs a new TurnUsage.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.ITurnUsage);

            /** TurnUsage model. */
            public model: string;

            /** TurnUsage inputTokens. */
            public inputTokens: (number|Long);

            /** TurnUsage outputTokens. */
            public outputTokens: (number|Long);

            /** TurnUsage cacheReadTokens. */
            public cacheReadTokens: (number|Long);

            /** TurnUsage cacheWriteTokens. */
            public cacheWriteTokens: (number|Long);

            /** TurnUsage tokensKnown. */
            public tokensKnown: boolean;

            /** TurnUsage cacheHitPercent. */
            public cacheHitPercent: number;

            /** TurnUsage costKnown. */
            public costKnown: boolean;

            /** TurnUsage costUsd. */
            public costUsd: number;

            /** TurnUsage costSource. */
            public costSource: string;

            /** TurnUsage pricingDate. */
            public pricingDate: string;

            /** TurnUsage changes. */
            public changes: muster.v1.ITurnFileChange[];

            /** TurnUsage changesIncomplete. */
            public changesIncomplete: boolean;

            /** TurnUsage cacheHitKnown. */
            public cacheHitKnown: boolean;

            /** TurnUsage requestCount. */
            public requestCount: (number|Long);

            /** TurnUsage firstRequestCacheHitKnown. */
            public firstRequestCacheHitKnown: boolean;

            /** TurnUsage firstRequestCacheHitPercent. */
            public firstRequestCacheHitPercent: number;

            /** TurnUsage cacheExpiry. */
            public cacheExpiry?: (muster.v1.ICacheExpiry|null);

            /**
             * Creates a new TurnUsage instance using the specified properties.
             * @param [properties] Properties to set
             * @returns TurnUsage instance
             */
            public static create(properties?: muster.v1.ITurnUsage): muster.v1.TurnUsage;

            /**
             * Encodes the specified TurnUsage message. Does not implicitly {@link muster.v1.TurnUsage.verify|verify} messages.
             * @param message TurnUsage message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.ITurnUsage, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a TurnUsage message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns TurnUsage
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.TurnUsage;

            /**
             * Gets the default type url for TurnUsage
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a CacheExpiry. */
        interface ICacheExpiry {

            /** CacheExpiry fromUnix */
            fromUnix?: (number|Long|null);

            /** CacheExpiry untilUnix */
            untilUnix?: (number|Long|null);

            /** CacheExpiry label */
            label?: (string|null);

            /** CacheExpiry detail */
            detail?: (string|null);

            /** CacheExpiry observedGapSeconds */
            observedGapSeconds?: (number|Long|null);
        }

        /** Represents a CacheExpiry. */
        class CacheExpiry implements ICacheExpiry {

            /**
             * Constructs a new CacheExpiry.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.ICacheExpiry);

            /** CacheExpiry fromUnix. */
            public fromUnix: (number|Long);

            /** CacheExpiry untilUnix. */
            public untilUnix: (number|Long);

            /** CacheExpiry label. */
            public label: string;

            /** CacheExpiry detail. */
            public detail: string;

            /** CacheExpiry observedGapSeconds. */
            public observedGapSeconds: (number|Long);

            /**
             * Creates a new CacheExpiry instance using the specified properties.
             * @param [properties] Properties to set
             * @returns CacheExpiry instance
             */
            public static create(properties?: muster.v1.ICacheExpiry): muster.v1.CacheExpiry;

            /**
             * Encodes the specified CacheExpiry message. Does not implicitly {@link muster.v1.CacheExpiry.verify|verify} messages.
             * @param message CacheExpiry message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.ICacheExpiry, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a CacheExpiry message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns CacheExpiry
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.CacheExpiry;

            /**
             * Gets the default type url for CacheExpiry
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a TurnFileChange. */
        interface ITurnFileChange {

            /** TurnFileChange path */
            path?: (string|null);

            /** TurnFileChange previousPath */
            previousPath?: (string|null);

            /** TurnFileChange kind */
            kind?: (string|null);

            /** TurnFileChange diff */
            diff?: (string|null);

            /** TurnFileChange additions */
            additions?: (number|null);

            /** TurnFileChange deletions */
            deletions?: (number|null);

            /** TurnFileChange countsKnown */
            countsKnown?: (boolean|null);

            /** TurnFileChange truncated */
            truncated?: (boolean|null);

            /** TurnFileChange openPath */
            openPath?: (string|null);
        }

        /** Represents a TurnFileChange. */
        class TurnFileChange implements ITurnFileChange {

            /**
             * Constructs a new TurnFileChange.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.ITurnFileChange);

            /** TurnFileChange path. */
            public path: string;

            /** TurnFileChange previousPath. */
            public previousPath: string;

            /** TurnFileChange kind. */
            public kind: string;

            /** TurnFileChange diff. */
            public diff: string;

            /** TurnFileChange additions. */
            public additions: number;

            /** TurnFileChange deletions. */
            public deletions: number;

            /** TurnFileChange countsKnown. */
            public countsKnown: boolean;

            /** TurnFileChange truncated. */
            public truncated: boolean;

            /** TurnFileChange openPath. */
            public openPath: string;

            /**
             * Creates a new TurnFileChange instance using the specified properties.
             * @param [properties] Properties to set
             * @returns TurnFileChange instance
             */
            public static create(properties?: muster.v1.ITurnFileChange): muster.v1.TurnFileChange;

            /**
             * Encodes the specified TurnFileChange message. Does not implicitly {@link muster.v1.TurnFileChange.verify|verify} messages.
             * @param message TurnFileChange message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.ITurnFileChange, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a TurnFileChange message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns TurnFileChange
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.TurnFileChange;

            /**
             * Gets the default type url for TurnFileChange
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of an AgentMessagesRequest. */
        interface IAgentMessagesRequest {

            /** AgentMessagesRequest session */
            session?: (string|null);

            /** AgentMessagesRequest tail */
            tail?: (number|null);

            /** AgentMessagesRequest beforeId */
            beforeId?: (string|null);

            /** AgentMessagesRequest afterId */
            afterId?: (string|null);

            /** AgentMessagesRequest firstId */
            firstId?: (string|null);

            /** AgentMessagesRequest prefixHash */
            prefixHash?: (Uint8Array|null);
        }

        /** Represents an AgentMessagesRequest. */
        class AgentMessagesRequest implements IAgentMessagesRequest {

            /**
             * Constructs a new AgentMessagesRequest.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.IAgentMessagesRequest);

            /** AgentMessagesRequest session. */
            public session: string;

            /** AgentMessagesRequest tail. */
            public tail: number;

            /** AgentMessagesRequest beforeId. */
            public beforeId: string;

            /** AgentMessagesRequest afterId. */
            public afterId: string;

            /** AgentMessagesRequest firstId. */
            public firstId: string;

            /** AgentMessagesRequest prefixHash. */
            public prefixHash: Uint8Array;

            /**
             * Creates a new AgentMessagesRequest instance using the specified properties.
             * @param [properties] Properties to set
             * @returns AgentMessagesRequest instance
             */
            public static create(properties?: muster.v1.IAgentMessagesRequest): muster.v1.AgentMessagesRequest;

            /**
             * Encodes the specified AgentMessagesRequest message. Does not implicitly {@link muster.v1.AgentMessagesRequest.verify|verify} messages.
             * @param message AgentMessagesRequest message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.IAgentMessagesRequest, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes an AgentMessagesRequest message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns AgentMessagesRequest
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.AgentMessagesRequest;

            /**
             * Gets the default type url for AgentMessagesRequest
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of an AgentMessagePage. */
        interface IAgentMessagePage {

            /** AgentMessagePage session */
            session?: (string|null);

            /** AgentMessagePage messages */
            messages?: (muster.v1.IAgentMessage[]|null);

            /** AgentMessagePage hasMore */
            hasMore?: (boolean|null);

            /** AgentMessagePage reset */
            reset?: (boolean|null);

            /** AgentMessagePage hasNewer */
            hasNewer?: (boolean|null);
        }

        /** Represents an AgentMessagePage. */
        class AgentMessagePage implements IAgentMessagePage {

            /**
             * Constructs a new AgentMessagePage.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.IAgentMessagePage);

            /** AgentMessagePage session. */
            public session: string;

            /** AgentMessagePage messages. */
            public messages: muster.v1.IAgentMessage[];

            /** AgentMessagePage hasMore. */
            public hasMore: boolean;

            /** AgentMessagePage reset. */
            public reset: boolean;

            /** AgentMessagePage hasNewer. */
            public hasNewer: boolean;

            /**
             * Creates a new AgentMessagePage instance using the specified properties.
             * @param [properties] Properties to set
             * @returns AgentMessagePage instance
             */
            public static create(properties?: muster.v1.IAgentMessagePage): muster.v1.AgentMessagePage;

            /**
             * Encodes the specified AgentMessagePage message. Does not implicitly {@link muster.v1.AgentMessagePage.verify|verify} messages.
             * @param message AgentMessagePage message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.IAgentMessagePage, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes an AgentMessagePage message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns AgentMessagePage
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.AgentMessagePage;

            /**
             * Gets the default type url for AgentMessagePage
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a PendingInteraction. */
        interface IPendingInteraction {

            /** PendingInteraction id */
            id?: (string|null);

            /** PendingInteraction session */
            session?: (string|null);

            /** PendingInteraction kind */
            kind?: (string|null);

            /** PendingInteraction prompt */
            prompt?: (string|null);

            /** PendingInteraction options */
            options?: (string[]|null);

            /** PendingInteraction questions */
            questions?: (muster.v1.IInteractionQuestion[]|null);

            /** PendingInteraction optional */
            optional?: (boolean|null);
        }

        /** Represents a PendingInteraction. */
        class PendingInteraction implements IPendingInteraction {

            /**
             * Constructs a new PendingInteraction.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.IPendingInteraction);

            /** PendingInteraction id. */
            public id: string;

            /** PendingInteraction session. */
            public session: string;

            /** PendingInteraction kind. */
            public kind: string;

            /** PendingInteraction prompt. */
            public prompt: string;

            /** PendingInteraction options. */
            public options: string[];

            /** PendingInteraction questions. */
            public questions: muster.v1.IInteractionQuestion[];

            /** PendingInteraction optional. */
            public optional: boolean;

            /**
             * Creates a new PendingInteraction instance using the specified properties.
             * @param [properties] Properties to set
             * @returns PendingInteraction instance
             */
            public static create(properties?: muster.v1.IPendingInteraction): muster.v1.PendingInteraction;

            /**
             * Encodes the specified PendingInteraction message. Does not implicitly {@link muster.v1.PendingInteraction.verify|verify} messages.
             * @param message PendingInteraction message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.IPendingInteraction, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a PendingInteraction message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns PendingInteraction
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.PendingInteraction;

            /**
             * Gets the default type url for PendingInteraction
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of an InteractionQuestion. */
        interface IInteractionQuestion {

            /** InteractionQuestion id */
            id?: (string|null);

            /** InteractionQuestion header */
            header?: (string|null);

            /** InteractionQuestion prompt */
            prompt?: (string|null);

            /** InteractionQuestion options */
            options?: (muster.v1.IQuestionOption[]|null);

            /** InteractionQuestion multiSelect */
            multiSelect?: (boolean|null);

            /** InteractionQuestion allowText */
            allowText?: (boolean|null);
        }

        /** Represents an InteractionQuestion. */
        class InteractionQuestion implements IInteractionQuestion {

            /**
             * Constructs a new InteractionQuestion.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.IInteractionQuestion);

            /** InteractionQuestion id. */
            public id: string;

            /** InteractionQuestion header. */
            public header: string;

            /** InteractionQuestion prompt. */
            public prompt: string;

            /** InteractionQuestion options. */
            public options: muster.v1.IQuestionOption[];

            /** InteractionQuestion multiSelect. */
            public multiSelect: boolean;

            /** InteractionQuestion allowText. */
            public allowText: boolean;

            /**
             * Creates a new InteractionQuestion instance using the specified properties.
             * @param [properties] Properties to set
             * @returns InteractionQuestion instance
             */
            public static create(properties?: muster.v1.IInteractionQuestion): muster.v1.InteractionQuestion;

            /**
             * Encodes the specified InteractionQuestion message. Does not implicitly {@link muster.v1.InteractionQuestion.verify|verify} messages.
             * @param message InteractionQuestion message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.IInteractionQuestion, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes an InteractionQuestion message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns InteractionQuestion
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.InteractionQuestion;

            /**
             * Gets the default type url for InteractionQuestion
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a QuestionOption. */
        interface IQuestionOption {

            /** QuestionOption id */
            id?: (string|null);

            /** QuestionOption label */
            label?: (string|null);

            /** QuestionOption description */
            description?: (string|null);
        }

        /** Represents a QuestionOption. */
        class QuestionOption implements IQuestionOption {

            /**
             * Constructs a new QuestionOption.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.IQuestionOption);

            /** QuestionOption id. */
            public id: string;

            /** QuestionOption label. */
            public label: string;

            /** QuestionOption description. */
            public description: string;

            /**
             * Creates a new QuestionOption instance using the specified properties.
             * @param [properties] Properties to set
             * @returns QuestionOption instance
             */
            public static create(properties?: muster.v1.IQuestionOption): muster.v1.QuestionOption;

            /**
             * Encodes the specified QuestionOption message. Does not implicitly {@link muster.v1.QuestionOption.verify|verify} messages.
             * @param message QuestionOption message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.IQuestionOption, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a QuestionOption message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns QuestionOption
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.QuestionOption;

            /**
             * Gets the default type url for QuestionOption
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a QuestionAnswer. */
        interface IQuestionAnswer {

            /** QuestionAnswer questionId */
            questionId?: (string|null);

            /** QuestionAnswer optionIds */
            optionIds?: (string[]|null);

            /** QuestionAnswer text */
            text?: (string|null);
        }

        /** Represents a QuestionAnswer. */
        class QuestionAnswer implements IQuestionAnswer {

            /**
             * Constructs a new QuestionAnswer.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.IQuestionAnswer);

            /** QuestionAnswer questionId. */
            public questionId: string;

            /** QuestionAnswer optionIds. */
            public optionIds: string[];

            /** QuestionAnswer text. */
            public text: string;

            /**
             * Creates a new QuestionAnswer instance using the specified properties.
             * @param [properties] Properties to set
             * @returns QuestionAnswer instance
             */
            public static create(properties?: muster.v1.IQuestionAnswer): muster.v1.QuestionAnswer;

            /**
             * Encodes the specified QuestionAnswer message. Does not implicitly {@link muster.v1.QuestionAnswer.verify|verify} messages.
             * @param message QuestionAnswer message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.IQuestionAnswer, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a QuestionAnswer message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns QuestionAnswer
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.QuestionAnswer;

            /**
             * Gets the default type url for QuestionAnswer
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of an AgentEvent. */
        interface IAgentEvent {

            /** AgentEvent listChanged */
            listChanged?: (muster.v1.IAgentSessionPage|null);

            /** AgentEvent message */
            message?: (muster.v1.IAgentMessage|null);

            /** AgentEvent state */
            state?: (muster.v1.IAgentSession|null);

            /** AgentEvent interaction */
            interaction?: (muster.v1.IPendingInteraction|null);

            /** AgentEvent interactionResolved */
            interactionResolved?: (string|null);
        }

        /** Represents an AgentEvent. */
        class AgentEvent implements IAgentEvent {

            /**
             * Constructs a new AgentEvent.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.IAgentEvent);

            /** AgentEvent listChanged. */
            public listChanged?: (muster.v1.IAgentSessionPage|null);

            /** AgentEvent message. */
            public message?: (muster.v1.IAgentMessage|null);

            /** AgentEvent state. */
            public state?: (muster.v1.IAgentSession|null);

            /** AgentEvent interaction. */
            public interaction?: (muster.v1.IPendingInteraction|null);

            /** AgentEvent interactionResolved. */
            public interactionResolved?: (string|null);

            /** AgentEvent ev. */
            public ev?: ("listChanged"|"message"|"state"|"interaction"|"interactionResolved");

            /**
             * Creates a new AgentEvent instance using the specified properties.
             * @param [properties] Properties to set
             * @returns AgentEvent instance
             */
            public static create(properties?: muster.v1.IAgentEvent): muster.v1.AgentEvent;

            /**
             * Encodes the specified AgentEvent message. Does not implicitly {@link muster.v1.AgentEvent.verify|verify} messages.
             * @param message AgentEvent message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.IAgentEvent, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes an AgentEvent message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns AgentEvent
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.AgentEvent;

            /**
             * Gets the default type url for AgentEvent
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of an AgentSendRequest. */
        interface IAgentSendRequest {

            /** AgentSendRequest opId */
            opId?: (string|null);

            /** AgentSendRequest session */
            session?: (string|null);

            /** AgentSendRequest text */
            text?: (string|null);

            /** AgentSendRequest attachments */
            attachments?: (muster.v1.IAgentAttachment[]|null);

            /** AgentSendRequest releaseUnconfirmed */
            releaseUnconfirmed?: (string|null);
        }

        /** Represents an AgentSendRequest. */
        class AgentSendRequest implements IAgentSendRequest {

            /**
             * Constructs a new AgentSendRequest.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.IAgentSendRequest);

            /** AgentSendRequest opId. */
            public opId: string;

            /** AgentSendRequest session. */
            public session: string;

            /** AgentSendRequest text. */
            public text: string;

            /** AgentSendRequest attachments. */
            public attachments: muster.v1.IAgentAttachment[];

            /** AgentSendRequest releaseUnconfirmed. */
            public releaseUnconfirmed: string;

            /**
             * Creates a new AgentSendRequest instance using the specified properties.
             * @param [properties] Properties to set
             * @returns AgentSendRequest instance
             */
            public static create(properties?: muster.v1.IAgentSendRequest): muster.v1.AgentSendRequest;

            /**
             * Encodes the specified AgentSendRequest message. Does not implicitly {@link muster.v1.AgentSendRequest.verify|verify} messages.
             * @param message AgentSendRequest message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.IAgentSendRequest, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes an AgentSendRequest message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns AgentSendRequest
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.AgentSendRequest;

            /**
             * Gets the default type url for AgentSendRequest
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of an AgentAttachment. */
        interface IAgentAttachment {

            /** AgentAttachment name */
            name?: (string|null);

            /** AgentAttachment mediaType */
            mediaType?: (string|null);

            /** AgentAttachment data */
            data?: (Uint8Array|null);
        }

        /** Represents an AgentAttachment. */
        class AgentAttachment implements IAgentAttachment {

            /**
             * Constructs a new AgentAttachment.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.IAgentAttachment);

            /** AgentAttachment name. */
            public name: string;

            /** AgentAttachment mediaType. */
            public mediaType: string;

            /** AgentAttachment data. */
            public data: Uint8Array;

            /**
             * Creates a new AgentAttachment instance using the specified properties.
             * @param [properties] Properties to set
             * @returns AgentAttachment instance
             */
            public static create(properties?: muster.v1.IAgentAttachment): muster.v1.AgentAttachment;

            /**
             * Encodes the specified AgentAttachment message. Does not implicitly {@link muster.v1.AgentAttachment.verify|verify} messages.
             * @param message AgentAttachment message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.IAgentAttachment, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes an AgentAttachment message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns AgentAttachment
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.AgentAttachment;

            /**
             * Gets the default type url for AgentAttachment
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of an AgentRespondRequest. */
        interface IAgentRespondRequest {

            /** AgentRespondRequest opId */
            opId?: (string|null);

            /** AgentRespondRequest interaction */
            interaction?: (string|null);

            /** AgentRespondRequest option */
            option?: (string|null);

            /** AgentRespondRequest answer */
            answer?: (string|null);

            /** AgentRespondRequest session */
            session?: (string|null);

            /** AgentRespondRequest answers */
            answers?: (muster.v1.IQuestionAnswer[]|null);
        }

        /** Represents an AgentRespondRequest. */
        class AgentRespondRequest implements IAgentRespondRequest {

            /**
             * Constructs a new AgentRespondRequest.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.IAgentRespondRequest);

            /** AgentRespondRequest opId. */
            public opId: string;

            /** AgentRespondRequest interaction. */
            public interaction: string;

            /** AgentRespondRequest option. */
            public option: string;

            /** AgentRespondRequest answer. */
            public answer: string;

            /** AgentRespondRequest session. */
            public session: string;

            /** AgentRespondRequest answers. */
            public answers: muster.v1.IQuestionAnswer[];

            /**
             * Creates a new AgentRespondRequest instance using the specified properties.
             * @param [properties] Properties to set
             * @returns AgentRespondRequest instance
             */
            public static create(properties?: muster.v1.IAgentRespondRequest): muster.v1.AgentRespondRequest;

            /**
             * Encodes the specified AgentRespondRequest message. Does not implicitly {@link muster.v1.AgentRespondRequest.verify|verify} messages.
             * @param message AgentRespondRequest message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.IAgentRespondRequest, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes an AgentRespondRequest message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns AgentRespondRequest
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.AgentRespondRequest;

            /**
             * Gets the default type url for AgentRespondRequest
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of an AgentInterruptRequest. */
        interface IAgentInterruptRequest {

            /** AgentInterruptRequest opId */
            opId?: (string|null);

            /** AgentInterruptRequest session */
            session?: (string|null);
        }

        /** Represents an AgentInterruptRequest. */
        class AgentInterruptRequest implements IAgentInterruptRequest {

            /**
             * Constructs a new AgentInterruptRequest.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.IAgentInterruptRequest);

            /** AgentInterruptRequest opId. */
            public opId: string;

            /** AgentInterruptRequest session. */
            public session: string;

            /**
             * Creates a new AgentInterruptRequest instance using the specified properties.
             * @param [properties] Properties to set
             * @returns AgentInterruptRequest instance
             */
            public static create(properties?: muster.v1.IAgentInterruptRequest): muster.v1.AgentInterruptRequest;

            /**
             * Encodes the specified AgentInterruptRequest message. Does not implicitly {@link muster.v1.AgentInterruptRequest.verify|verify} messages.
             * @param message AgentInterruptRequest message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.IAgentInterruptRequest, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes an AgentInterruptRequest message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns AgentInterruptRequest
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.AgentInterruptRequest;

            /**
             * Gets the default type url for AgentInterruptRequest
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of an AgentFilesRequest. */
        interface IAgentFilesRequest {

            /** AgentFilesRequest session */
            session?: (string|null);

            /** AgentFilesRequest path */
            path?: (string|null);
        }

        /** Represents an AgentFilesRequest. */
        class AgentFilesRequest implements IAgentFilesRequest {

            /**
             * Constructs a new AgentFilesRequest.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.IAgentFilesRequest);

            /** AgentFilesRequest session. */
            public session: string;

            /** AgentFilesRequest path. */
            public path: string;

            /**
             * Creates a new AgentFilesRequest instance using the specified properties.
             * @param [properties] Properties to set
             * @returns AgentFilesRequest instance
             */
            public static create(properties?: muster.v1.IAgentFilesRequest): muster.v1.AgentFilesRequest;

            /**
             * Encodes the specified AgentFilesRequest message. Does not implicitly {@link muster.v1.AgentFilesRequest.verify|verify} messages.
             * @param message AgentFilesRequest message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.IAgentFilesRequest, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes an AgentFilesRequest message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns AgentFilesRequest
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.AgentFilesRequest;

            /**
             * Gets the default type url for AgentFilesRequest
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a FileEntry. */
        interface IFileEntry {

            /** FileEntry name */
            name?: (string|null);

            /** FileEntry dir */
            dir?: (boolean|null);

            /** FileEntry size */
            size?: (number|Long|null);

            /** FileEntry modifiedUnix */
            modifiedUnix?: (number|Long|null);
        }

        /** Represents a FileEntry. */
        class FileEntry implements IFileEntry {

            /**
             * Constructs a new FileEntry.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.IFileEntry);

            /** FileEntry name. */
            public name: string;

            /** FileEntry dir. */
            public dir: boolean;

            /** FileEntry size. */
            public size: (number|Long);

            /** FileEntry modifiedUnix. */
            public modifiedUnix: (number|Long);

            /**
             * Creates a new FileEntry instance using the specified properties.
             * @param [properties] Properties to set
             * @returns FileEntry instance
             */
            public static create(properties?: muster.v1.IFileEntry): muster.v1.FileEntry;

            /**
             * Encodes the specified FileEntry message. Does not implicitly {@link muster.v1.FileEntry.verify|verify} messages.
             * @param message FileEntry message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.IFileEntry, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a FileEntry message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns FileEntry
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.FileEntry;

            /**
             * Gets the default type url for FileEntry
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a FileList. */
        interface IFileList {

            /** FileList path */
            path?: (string|null);

            /** FileList entries */
            entries?: (muster.v1.IFileEntry[]|null);
        }

        /** Represents a FileList. */
        class FileList implements IFileList {

            /**
             * Constructs a new FileList.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.IFileList);

            /** FileList path. */
            public path: string;

            /** FileList entries. */
            public entries: muster.v1.IFileEntry[];

            /**
             * Creates a new FileList instance using the specified properties.
             * @param [properties] Properties to set
             * @returns FileList instance
             */
            public static create(properties?: muster.v1.IFileList): muster.v1.FileList;

            /**
             * Encodes the specified FileList message. Does not implicitly {@link muster.v1.FileList.verify|verify} messages.
             * @param message FileList message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.IFileList, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a FileList message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns FileList
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.FileList;

            /**
             * Gets the default type url for FileList
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of an AgentFileRequest. */
        interface IAgentFileRequest {

            /** AgentFileRequest session */
            session?: (string|null);

            /** AgentFileRequest path */
            path?: (string|null);

            /** AgentFileRequest maxBytes */
            maxBytes?: (number|Long|null);
        }

        /** Represents an AgentFileRequest. */
        class AgentFileRequest implements IAgentFileRequest {

            /**
             * Constructs a new AgentFileRequest.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.IAgentFileRequest);

            /** AgentFileRequest session. */
            public session: string;

            /** AgentFileRequest path. */
            public path: string;

            /** AgentFileRequest maxBytes. */
            public maxBytes: (number|Long);

            /**
             * Creates a new AgentFileRequest instance using the specified properties.
             * @param [properties] Properties to set
             * @returns AgentFileRequest instance
             */
            public static create(properties?: muster.v1.IAgentFileRequest): muster.v1.AgentFileRequest;

            /**
             * Encodes the specified AgentFileRequest message. Does not implicitly {@link muster.v1.AgentFileRequest.verify|verify} messages.
             * @param message AgentFileRequest message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.IAgentFileRequest, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes an AgentFileRequest message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns AgentFileRequest
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.AgentFileRequest;

            /**
             * Gets the default type url for AgentFileRequest
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a FileContent. */
        interface IFileContent {

            /** FileContent path */
            path?: (string|null);

            /** FileContent content */
            content?: (Uint8Array|null);

            /** FileContent truncated */
            truncated?: (boolean|null);
        }

        /** Represents a FileContent. */
        class FileContent implements IFileContent {

            /**
             * Constructs a new FileContent.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.IFileContent);

            /** FileContent path. */
            public path: string;

            /** FileContent content. */
            public content: Uint8Array;

            /** FileContent truncated. */
            public truncated: boolean;

            /**
             * Creates a new FileContent instance using the specified properties.
             * @param [properties] Properties to set
             * @returns FileContent instance
             */
            public static create(properties?: muster.v1.IFileContent): muster.v1.FileContent;

            /**
             * Encodes the specified FileContent message. Does not implicitly {@link muster.v1.FileContent.verify|verify} messages.
             * @param message FileContent message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.IFileContent, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a FileContent message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns FileContent
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.FileContent;

            /**
             * Gets the default type url for FileContent
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of an AgentGitRequest. */
        interface IAgentGitRequest {

            /** AgentGitRequest session */
            session?: (string|null);

            /** AgentGitRequest limit */
            limit?: (number|null);

            /** AgentGitRequest diffCommit */
            diffCommit?: (string|null);
        }

        /** Represents an AgentGitRequest. */
        class AgentGitRequest implements IAgentGitRequest {

            /**
             * Constructs a new AgentGitRequest.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.IAgentGitRequest);

            /** AgentGitRequest session. */
            public session: string;

            /** AgentGitRequest limit. */
            public limit: number;

            /** AgentGitRequest diffCommit. */
            public diffCommit: string;

            /**
             * Creates a new AgentGitRequest instance using the specified properties.
             * @param [properties] Properties to set
             * @returns AgentGitRequest instance
             */
            public static create(properties?: muster.v1.IAgentGitRequest): muster.v1.AgentGitRequest;

            /**
             * Encodes the specified AgentGitRequest message. Does not implicitly {@link muster.v1.AgentGitRequest.verify|verify} messages.
             * @param message AgentGitRequest message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.IAgentGitRequest, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes an AgentGitRequest message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns AgentGitRequest
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.AgentGitRequest;

            /**
             * Gets the default type url for AgentGitRequest
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a GitCommit. */
        interface IGitCommit {

            /** GitCommit hash */
            hash?: (string|null);

            /** GitCommit author */
            author?: (string|null);

            /** GitCommit tsUnix */
            tsUnix?: (number|Long|null);

            /** GitCommit subject */
            subject?: (string|null);
        }

        /** Represents a GitCommit. */
        class GitCommit implements IGitCommit {

            /**
             * Constructs a new GitCommit.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.IGitCommit);

            /** GitCommit hash. */
            public hash: string;

            /** GitCommit author. */
            public author: string;

            /** GitCommit tsUnix. */
            public tsUnix: (number|Long);

            /** GitCommit subject. */
            public subject: string;

            /**
             * Creates a new GitCommit instance using the specified properties.
             * @param [properties] Properties to set
             * @returns GitCommit instance
             */
            public static create(properties?: muster.v1.IGitCommit): muster.v1.GitCommit;

            /**
             * Encodes the specified GitCommit message. Does not implicitly {@link muster.v1.GitCommit.verify|verify} messages.
             * @param message GitCommit message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.IGitCommit, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a GitCommit message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns GitCommit
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.GitCommit;

            /**
             * Gets the default type url for GitCommit
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a GitLog. */
        interface IGitLog {

            /** GitLog commits */
            commits?: (muster.v1.IGitCommit[]|null);

            /** GitLog diff */
            diff?: (string|null);
        }

        /** Represents a GitLog. */
        class GitLog implements IGitLog {

            /**
             * Constructs a new GitLog.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.IGitLog);

            /** GitLog commits. */
            public commits: muster.v1.IGitCommit[];

            /** GitLog diff. */
            public diff: string;

            /**
             * Creates a new GitLog instance using the specified properties.
             * @param [properties] Properties to set
             * @returns GitLog instance
             */
            public static create(properties?: muster.v1.IGitLog): muster.v1.GitLog;

            /**
             * Encodes the specified GitLog message. Does not implicitly {@link muster.v1.GitLog.verify|verify} messages.
             * @param message GitLog message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.IGitLog, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a GitLog message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns GitLog
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.GitLog;

            /**
             * Gets the default type url for GitLog
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of an AgentChatsRequest. */
        interface IAgentChatsRequest {

            /** AgentChatsRequest provider */
            provider?: (string|null);

            /** AgentChatsRequest fullHistory */
            fullHistory?: (boolean|null);

            /** AgentChatsRequest opId */
            opId?: (string|null);
        }

        /** Represents an AgentChatsRequest. */
        class AgentChatsRequest implements IAgentChatsRequest {

            /**
             * Constructs a new AgentChatsRequest.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.IAgentChatsRequest);

            /** AgentChatsRequest provider. */
            public provider: string;

            /** AgentChatsRequest fullHistory. */
            public fullHistory: boolean;

            /** AgentChatsRequest opId. */
            public opId: string;

            /**
             * Creates a new AgentChatsRequest instance using the specified properties.
             * @param [properties] Properties to set
             * @returns AgentChatsRequest instance
             */
            public static create(properties?: muster.v1.IAgentChatsRequest): muster.v1.AgentChatsRequest;

            /**
             * Encodes the specified AgentChatsRequest message. Does not implicitly {@link muster.v1.AgentChatsRequest.verify|verify} messages.
             * @param message AgentChatsRequest message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.IAgentChatsRequest, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes an AgentChatsRequest message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns AgentChatsRequest
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.AgentChatsRequest;

            /**
             * Gets the default type url for AgentChatsRequest
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of an AgentChatCatalog. */
        interface IAgentChatCatalog {

            /** AgentChatCatalog provider */
            provider?: (string|null);

            /** AgentChatCatalog appRunning */
            appRunning?: (boolean|null);

            /** AgentChatCatalog chatMode */
            chatMode?: (boolean|null);

            /** AgentChatCatalog chats */
            chats?: (muster.v1.IAgentChat[]|null);

            /** AgentChatCatalog current */
            current?: (string|null);

            /** AgentChatCatalog newSession */
            newSession?: (string|null);

            /** AgentChatCatalog busy */
            busy?: (string|null);

            /** AgentChatCatalog historyComplete */
            historyComplete?: (boolean|null);

            /** AgentChatCatalog historyWarning */
            historyWarning?: (string|null);

            /** AgentChatCatalog agentWorking */
            agentWorking?: (boolean|null);
        }

        /** Represents an AgentChatCatalog. */
        class AgentChatCatalog implements IAgentChatCatalog {

            /**
             * Constructs a new AgentChatCatalog.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.IAgentChatCatalog);

            /** AgentChatCatalog provider. */
            public provider: string;

            /** AgentChatCatalog appRunning. */
            public appRunning: boolean;

            /** AgentChatCatalog chatMode. */
            public chatMode: boolean;

            /** AgentChatCatalog chats. */
            public chats: muster.v1.IAgentChat[];

            /** AgentChatCatalog current. */
            public current: string;

            /** AgentChatCatalog newSession. */
            public newSession: string;

            /** AgentChatCatalog busy. */
            public busy: string;

            /** AgentChatCatalog historyComplete. */
            public historyComplete: boolean;

            /** AgentChatCatalog historyWarning. */
            public historyWarning: string;

            /** AgentChatCatalog agentWorking. */
            public agentWorking: boolean;

            /**
             * Creates a new AgentChatCatalog instance using the specified properties.
             * @param [properties] Properties to set
             * @returns AgentChatCatalog instance
             */
            public static create(properties?: muster.v1.IAgentChatCatalog): muster.v1.AgentChatCatalog;

            /**
             * Encodes the specified AgentChatCatalog message. Does not implicitly {@link muster.v1.AgentChatCatalog.verify|verify} messages.
             * @param message AgentChatCatalog message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.IAgentChatCatalog, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes an AgentChatCatalog message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns AgentChatCatalog
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.AgentChatCatalog;

            /**
             * Gets the default type url for AgentChatCatalog
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of an AgentChat. */
        interface IAgentChat {

            /** AgentChat id */
            id?: (string|null);

            /** AgentChat title */
            title?: (string|null);

            /** AgentChat current */
            current?: (boolean|null);
        }

        /** Represents an AgentChat. */
        class AgentChat implements IAgentChat {

            /**
             * Constructs a new AgentChat.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.IAgentChat);

            /** AgentChat id. */
            public id: string;

            /** AgentChat title. */
            public title: string;

            /** AgentChat current. */
            public current: boolean;

            /**
             * Creates a new AgentChat instance using the specified properties.
             * @param [properties] Properties to set
             * @returns AgentChat instance
             */
            public static create(properties?: muster.v1.IAgentChat): muster.v1.AgentChat;

            /**
             * Encodes the specified AgentChat message. Does not implicitly {@link muster.v1.AgentChat.verify|verify} messages.
             * @param message AgentChat message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.IAgentChat, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes an AgentChat message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns AgentChat
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.AgentChat;

            /**
             * Gets the default type url for AgentChat
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of an AgentChatOpenRequest. */
        interface IAgentChatOpenRequest {

            /** AgentChatOpenRequest provider */
            provider?: (string|null);

            /** AgentChatOpenRequest chat */
            chat?: (string|null);

            /** AgentChatOpenRequest opId */
            opId?: (string|null);
        }

        /** Represents an AgentChatOpenRequest. */
        class AgentChatOpenRequest implements IAgentChatOpenRequest {

            /**
             * Constructs a new AgentChatOpenRequest.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.IAgentChatOpenRequest);

            /** AgentChatOpenRequest provider. */
            public provider: string;

            /** AgentChatOpenRequest chat. */
            public chat: string;

            /** AgentChatOpenRequest opId. */
            public opId: string;

            /**
             * Creates a new AgentChatOpenRequest instance using the specified properties.
             * @param [properties] Properties to set
             * @returns AgentChatOpenRequest instance
             */
            public static create(properties?: muster.v1.IAgentChatOpenRequest): muster.v1.AgentChatOpenRequest;

            /**
             * Encodes the specified AgentChatOpenRequest message. Does not implicitly {@link muster.v1.AgentChatOpenRequest.verify|verify} messages.
             * @param message AgentChatOpenRequest message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.IAgentChatOpenRequest, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes an AgentChatOpenRequest message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns AgentChatOpenRequest
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.AgentChatOpenRequest;

            /**
             * Gets the default type url for AgentChatOpenRequest
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of an AgentChatOption. */
        interface IAgentChatOption {

            /** AgentChatOption name */
            name?: (string|null);

            /** AgentChatOption detail */
            detail?: (string|null);

            /** AgentChatOption selected */
            selected?: (boolean|null);
        }

        /** Represents an AgentChatOption. */
        class AgentChatOption implements IAgentChatOption {

            /**
             * Constructs a new AgentChatOption.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.IAgentChatOption);

            /** AgentChatOption name. */
            public name: string;

            /** AgentChatOption detail. */
            public detail: string;

            /** AgentChatOption selected. */
            public selected: boolean;

            /**
             * Creates a new AgentChatOption instance using the specified properties.
             * @param [properties] Properties to set
             * @returns AgentChatOption instance
             */
            public static create(properties?: muster.v1.IAgentChatOption): muster.v1.AgentChatOption;

            /**
             * Encodes the specified AgentChatOption message. Does not implicitly {@link muster.v1.AgentChatOption.verify|verify} messages.
             * @param message AgentChatOption message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.IAgentChatOption, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes an AgentChatOption message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns AgentChatOption
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.AgentChatOption;

            /**
             * Gets the default type url for AgentChatOption
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of an AgentChatOptions. */
        interface IAgentChatOptions {

            /** AgentChatOptions session */
            session?: (string|null);

            /** AgentChatOptions models */
            models?: (muster.v1.IAgentChatOption[]|null);

            /** AgentChatOptions efforts */
            efforts?: (muster.v1.IAgentChatOption[]|null);

            /** AgentChatOptions effort */
            effort?: (string|null);

            /** AgentChatOptions note */
            note?: (string|null);
        }

        /** Represents an AgentChatOptions. */
        class AgentChatOptions implements IAgentChatOptions {

            /**
             * Constructs a new AgentChatOptions.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.IAgentChatOptions);

            /** AgentChatOptions session. */
            public session: string;

            /** AgentChatOptions models. */
            public models: muster.v1.IAgentChatOption[];

            /** AgentChatOptions efforts. */
            public efforts: muster.v1.IAgentChatOption[];

            /** AgentChatOptions effort. */
            public effort: string;

            /** AgentChatOptions note. */
            public note: string;

            /**
             * Creates a new AgentChatOptions instance using the specified properties.
             * @param [properties] Properties to set
             * @returns AgentChatOptions instance
             */
            public static create(properties?: muster.v1.IAgentChatOptions): muster.v1.AgentChatOptions;

            /**
             * Encodes the specified AgentChatOptions message. Does not implicitly {@link muster.v1.AgentChatOptions.verify|verify} messages.
             * @param message AgentChatOptions message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.IAgentChatOptions, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes an AgentChatOptions message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns AgentChatOptions
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.AgentChatOptions;

            /**
             * Gets the default type url for AgentChatOptions
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of an AgentChatOptionsRequest. */
        interface IAgentChatOptionsRequest {

            /** AgentChatOptionsRequest session */
            session?: (string|null);

            /** AgentChatOptionsRequest opId */
            opId?: (string|null);
        }

        /** Represents an AgentChatOptionsRequest. */
        class AgentChatOptionsRequest implements IAgentChatOptionsRequest {

            /**
             * Constructs a new AgentChatOptionsRequest.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.IAgentChatOptionsRequest);

            /** AgentChatOptionsRequest session. */
            public session: string;

            /** AgentChatOptionsRequest opId. */
            public opId: string;

            /**
             * Creates a new AgentChatOptionsRequest instance using the specified properties.
             * @param [properties] Properties to set
             * @returns AgentChatOptionsRequest instance
             */
            public static create(properties?: muster.v1.IAgentChatOptionsRequest): muster.v1.AgentChatOptionsRequest;

            /**
             * Encodes the specified AgentChatOptionsRequest message. Does not implicitly {@link muster.v1.AgentChatOptionsRequest.verify|verify} messages.
             * @param message AgentChatOptionsRequest message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.IAgentChatOptionsRequest, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes an AgentChatOptionsRequest message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns AgentChatOptionsRequest
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.AgentChatOptionsRequest;

            /**
             * Gets the default type url for AgentChatOptionsRequest
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of an AgentChatSetOptionsRequest. */
        interface IAgentChatSetOptionsRequest {

            /** AgentChatSetOptionsRequest session */
            session?: (string|null);

            /** AgentChatSetOptionsRequest model */
            model?: (string|null);

            /** AgentChatSetOptionsRequest effort */
            effort?: (string|null);

            /** AgentChatSetOptionsRequest opId */
            opId?: (string|null);
        }

        /** Represents an AgentChatSetOptionsRequest. */
        class AgentChatSetOptionsRequest implements IAgentChatSetOptionsRequest {

            /**
             * Constructs a new AgentChatSetOptionsRequest.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.IAgentChatSetOptionsRequest);

            /** AgentChatSetOptionsRequest session. */
            public session: string;

            /** AgentChatSetOptionsRequest model. */
            public model: string;

            /** AgentChatSetOptionsRequest effort. */
            public effort: string;

            /** AgentChatSetOptionsRequest opId. */
            public opId: string;

            /**
             * Creates a new AgentChatSetOptionsRequest instance using the specified properties.
             * @param [properties] Properties to set
             * @returns AgentChatSetOptionsRequest instance
             */
            public static create(properties?: muster.v1.IAgentChatSetOptionsRequest): muster.v1.AgentChatSetOptionsRequest;

            /**
             * Encodes the specified AgentChatSetOptionsRequest message. Does not implicitly {@link muster.v1.AgentChatSetOptionsRequest.verify|verify} messages.
             * @param message AgentChatSetOptionsRequest message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.IAgentChatSetOptionsRequest, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes an AgentChatSetOptionsRequest message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns AgentChatSetOptionsRequest
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.AgentChatSetOptionsRequest;

            /**
             * Gets the default type url for AgentChatSetOptionsRequest
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a BrowserCallRequest. */
        interface IBrowserCallRequest {

            /** BrowserCallRequest opId */
            opId?: (string|null);

            /** BrowserCallRequest json */
            json?: (string|null);
        }

        /** Represents a BrowserCallRequest. */
        class BrowserCallRequest implements IBrowserCallRequest {

            /**
             * Constructs a new BrowserCallRequest.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.IBrowserCallRequest);

            /** BrowserCallRequest opId. */
            public opId: string;

            /** BrowserCallRequest json. */
            public json: string;

            /**
             * Creates a new BrowserCallRequest instance using the specified properties.
             * @param [properties] Properties to set
             * @returns BrowserCallRequest instance
             */
            public static create(properties?: muster.v1.IBrowserCallRequest): muster.v1.BrowserCallRequest;

            /**
             * Encodes the specified BrowserCallRequest message. Does not implicitly {@link muster.v1.BrowserCallRequest.verify|verify} messages.
             * @param message BrowserCallRequest message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.IBrowserCallRequest, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a BrowserCallRequest message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns BrowserCallRequest
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.BrowserCallRequest;

            /**
             * Gets the default type url for BrowserCallRequest
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a BrowserSetupRequest. */
        interface IBrowserSetupRequest {

            /** BrowserSetupRequest install */
            install?: (boolean|null);
        }

        /** Represents a BrowserSetupRequest. */
        class BrowserSetupRequest implements IBrowserSetupRequest {

            /**
             * Constructs a new BrowserSetupRequest.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.IBrowserSetupRequest);

            /** BrowserSetupRequest install. */
            public install: boolean;

            /**
             * Creates a new BrowserSetupRequest instance using the specified properties.
             * @param [properties] Properties to set
             * @returns BrowserSetupRequest instance
             */
            public static create(properties?: muster.v1.IBrowserSetupRequest): muster.v1.BrowserSetupRequest;

            /**
             * Encodes the specified BrowserSetupRequest message. Does not implicitly {@link muster.v1.BrowserSetupRequest.verify|verify} messages.
             * @param message BrowserSetupRequest message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.IBrowserSetupRequest, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a BrowserSetupRequest message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns BrowserSetupRequest
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.BrowserSetupRequest;

            /**
             * Gets the default type url for BrowserSetupRequest
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a BrowserStatus. */
        interface IBrowserStatus {

            /** BrowserStatus installed */
            installed?: (boolean|null);

            /** BrowserStatus connected */
            connected?: (boolean|null);

            /** BrowserStatus extensionPath */
            extensionPath?: (string|null);

            /** BrowserStatus message */
            message?: (string|null);
        }

        /** Represents a BrowserStatus. */
        class BrowserStatus implements IBrowserStatus {

            /**
             * Constructs a new BrowserStatus.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.IBrowserStatus);

            /** BrowserStatus installed. */
            public installed: boolean;

            /** BrowserStatus connected. */
            public connected: boolean;

            /** BrowserStatus extensionPath. */
            public extensionPath: string;

            /** BrowserStatus message. */
            public message: string;

            /**
             * Creates a new BrowserStatus instance using the specified properties.
             * @param [properties] Properties to set
             * @returns BrowserStatus instance
             */
            public static create(properties?: muster.v1.IBrowserStatus): muster.v1.BrowserStatus;

            /**
             * Encodes the specified BrowserStatus message. Does not implicitly {@link muster.v1.BrowserStatus.verify|verify} messages.
             * @param message BrowserStatus message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.IBrowserStatus, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a BrowserStatus message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns BrowserStatus
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.BrowserStatus;

            /**
             * Gets the default type url for BrowserStatus
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of an AgentTerminalRequest. */
        interface IAgentTerminalRequest {

            /** AgentTerminalRequest session */
            session?: (string|null);

            /** AgentTerminalRequest opId */
            opId?: (string|null);

            /** AgentTerminalRequest action */
            action?: (muster.v1.AgentTerminalRequest.Action|null);
        }

        /** Represents an AgentTerminalRequest. */
        class AgentTerminalRequest implements IAgentTerminalRequest {

            /**
             * Constructs a new AgentTerminalRequest.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.IAgentTerminalRequest);

            /** AgentTerminalRequest session. */
            public session: string;

            /** AgentTerminalRequest opId. */
            public opId: string;

            /** AgentTerminalRequest action. */
            public action: muster.v1.AgentTerminalRequest.Action;

            /**
             * Creates a new AgentTerminalRequest instance using the specified properties.
             * @param [properties] Properties to set
             * @returns AgentTerminalRequest instance
             */
            public static create(properties?: muster.v1.IAgentTerminalRequest): muster.v1.AgentTerminalRequest;

            /**
             * Encodes the specified AgentTerminalRequest message. Does not implicitly {@link muster.v1.AgentTerminalRequest.verify|verify} messages.
             * @param message AgentTerminalRequest message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.IAgentTerminalRequest, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes an AgentTerminalRequest message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns AgentTerminalRequest
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.AgentTerminalRequest;

            /**
             * Gets the default type url for AgentTerminalRequest
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        namespace AgentTerminalRequest {

            /** Action enum. */
            enum Action {
                START = 0,
                END = 1,
                RESTART = 2
            }
        }

        /** Properties of a TerminalRunRequest. */
        interface ITerminalRunRequest {

            /** TerminalRunRequest session */
            session?: (string|null);

            /** TerminalRunRequest opId */
            opId?: (string|null);

            /** TerminalRunRequest code */
            code?: (string|null);

            /** TerminalRunRequest language */
            language?: (string|null);

            /** TerminalRunRequest sourceMessage */
            sourceMessage?: (string|null);

            /** TerminalRunRequest cwd */
            cwd?: (string|null);

            /** TerminalRunRequest autoReturn */
            autoReturn?: (boolean|null);
        }

        /** Represents a TerminalRunRequest. */
        class TerminalRunRequest implements ITerminalRunRequest {

            /**
             * Constructs a new TerminalRunRequest.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.ITerminalRunRequest);

            /** TerminalRunRequest session. */
            public session: string;

            /** TerminalRunRequest opId. */
            public opId: string;

            /** TerminalRunRequest code. */
            public code: string;

            /** TerminalRunRequest language. */
            public language: string;

            /** TerminalRunRequest sourceMessage. */
            public sourceMessage: string;

            /** TerminalRunRequest cwd. */
            public cwd: string;

            /** TerminalRunRequest autoReturn. */
            public autoReturn: boolean;

            /**
             * Creates a new TerminalRunRequest instance using the specified properties.
             * @param [properties] Properties to set
             * @returns TerminalRunRequest instance
             */
            public static create(properties?: muster.v1.ITerminalRunRequest): muster.v1.TerminalRunRequest;

            /**
             * Encodes the specified TerminalRunRequest message. Does not implicitly {@link muster.v1.TerminalRunRequest.verify|verify} messages.
             * @param message TerminalRunRequest message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.ITerminalRunRequest, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a TerminalRunRequest message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns TerminalRunRequest
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.TerminalRunRequest;

            /**
             * Gets the default type url for TerminalRunRequest
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a TerminalRunsRequest. */
        interface ITerminalRunsRequest {

            /** TerminalRunsRequest session */
            session?: (string|null);
        }

        /** Represents a TerminalRunsRequest. */
        class TerminalRunsRequest implements ITerminalRunsRequest {

            /**
             * Constructs a new TerminalRunsRequest.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.ITerminalRunsRequest);

            /** TerminalRunsRequest session. */
            public session: string;

            /**
             * Creates a new TerminalRunsRequest instance using the specified properties.
             * @param [properties] Properties to set
             * @returns TerminalRunsRequest instance
             */
            public static create(properties?: muster.v1.ITerminalRunsRequest): muster.v1.TerminalRunsRequest;

            /**
             * Encodes the specified TerminalRunsRequest message. Does not implicitly {@link muster.v1.TerminalRunsRequest.verify|verify} messages.
             * @param message TerminalRunsRequest message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.ITerminalRunsRequest, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a TerminalRunsRequest message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns TerminalRunsRequest
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.TerminalRunsRequest;

            /**
             * Gets the default type url for TerminalRunsRequest
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a TerminalRunList. */
        interface ITerminalRunList {

            /** TerminalRunList runs */
            runs?: (muster.v1.ITerminalRun[]|null);
        }

        /** Represents a TerminalRunList. */
        class TerminalRunList implements ITerminalRunList {

            /**
             * Constructs a new TerminalRunList.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.ITerminalRunList);

            /** TerminalRunList runs. */
            public runs: muster.v1.ITerminalRun[];

            /**
             * Creates a new TerminalRunList instance using the specified properties.
             * @param [properties] Properties to set
             * @returns TerminalRunList instance
             */
            public static create(properties?: muster.v1.ITerminalRunList): muster.v1.TerminalRunList;

            /**
             * Encodes the specified TerminalRunList message. Does not implicitly {@link muster.v1.TerminalRunList.verify|verify} messages.
             * @param message TerminalRunList message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.ITerminalRunList, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a TerminalRunList message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns TerminalRunList
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.TerminalRunList;

            /**
             * Gets the default type url for TerminalRunList
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a TerminalRun. */
        interface ITerminalRun {

            /** TerminalRun id */
            id?: (string|null);

            /** TerminalRun session */
            session?: (string|null);

            /** TerminalRun code */
            code?: (string|null);

            /** TerminalRun language */
            language?: (string|null);

            /** TerminalRun sourceMessage */
            sourceMessage?: (string|null);

            /** TerminalRun cwd */
            cwd?: (string|null);

            /** TerminalRun state */
            state?: (string|null);

            /** TerminalRun output */
            output?: (string|null);

            /** TerminalRun truncated */
            truncated?: (boolean|null);

            /** TerminalRun exitKnown */
            exitKnown?: (boolean|null);

            /** TerminalRun exitCode */
            exitCode?: (number|null);

            /** TerminalRun startedUnix */
            startedUnix?: (number|Long|null);

            /** TerminalRun returnState */
            returnState?: (string|null);

            /** TerminalRun returnOwner */
            returnOwner?: (string|null);

            /** TerminalRun returnReady */
            returnReady?: (boolean|null);
        }

        /** Represents a TerminalRun. */
        class TerminalRun implements ITerminalRun {

            /**
             * Constructs a new TerminalRun.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.ITerminalRun);

            /** TerminalRun id. */
            public id: string;

            /** TerminalRun session. */
            public session: string;

            /** TerminalRun code. */
            public code: string;

            /** TerminalRun language. */
            public language: string;

            /** TerminalRun sourceMessage. */
            public sourceMessage: string;

            /** TerminalRun cwd. */
            public cwd: string;

            /** TerminalRun state. */
            public state: string;

            /** TerminalRun output. */
            public output: string;

            /** TerminalRun truncated. */
            public truncated: boolean;

            /** TerminalRun exitKnown. */
            public exitKnown: boolean;

            /** TerminalRun exitCode. */
            public exitCode: number;

            /** TerminalRun startedUnix. */
            public startedUnix: (number|Long);

            /** TerminalRun returnState. */
            public returnState: string;

            /** TerminalRun returnOwner. */
            public returnOwner: string;

            /** TerminalRun returnReady. */
            public returnReady: boolean;

            /**
             * Creates a new TerminalRun instance using the specified properties.
             * @param [properties] Properties to set
             * @returns TerminalRun instance
             */
            public static create(properties?: muster.v1.ITerminalRun): muster.v1.TerminalRun;

            /**
             * Encodes the specified TerminalRun message. Does not implicitly {@link muster.v1.TerminalRun.verify|verify} messages.
             * @param message TerminalRun message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.ITerminalRun, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a TerminalRun message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns TerminalRun
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.TerminalRun;

            /**
             * Gets the default type url for TerminalRun
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a DevicePresenceRequest. */
        interface IDevicePresenceRequest {

            /** DevicePresenceRequest publicKey */
            publicKey?: (Uint8Array|null);
        }

        /** Represents a DevicePresenceRequest. */
        class DevicePresenceRequest implements IDevicePresenceRequest {

            /**
             * Constructs a new DevicePresenceRequest.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.IDevicePresenceRequest);

            /** DevicePresenceRequest publicKey. */
            public publicKey: Uint8Array;

            /**
             * Creates a new DevicePresenceRequest instance using the specified properties.
             * @param [properties] Properties to set
             * @returns DevicePresenceRequest instance
             */
            public static create(properties?: muster.v1.IDevicePresenceRequest): muster.v1.DevicePresenceRequest;

            /**
             * Encodes the specified DevicePresenceRequest message. Does not implicitly {@link muster.v1.DevicePresenceRequest.verify|verify} messages.
             * @param message DevicePresenceRequest message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.IDevicePresenceRequest, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a DevicePresenceRequest message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns DevicePresenceRequest
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.DevicePresenceRequest;

            /**
             * Gets the default type url for DevicePresenceRequest
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a NetworkPresenceRequest. */
        interface INetworkPresenceRequest {

            /** NetworkPresenceRequest deviceId */
            deviceId?: (string|null);

            /** NetworkPresenceRequest grantLockedControl */
            grantLockedControl?: (boolean|null);

            /** NetworkPresenceRequest revoke */
            revoke?: (boolean|null);
        }

        /** Represents a NetworkPresenceRequest. */
        class NetworkPresenceRequest implements INetworkPresenceRequest {

            /**
             * Constructs a new NetworkPresenceRequest.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.INetworkPresenceRequest);

            /** NetworkPresenceRequest deviceId. */
            public deviceId: string;

            /** NetworkPresenceRequest grantLockedControl. */
            public grantLockedControl: boolean;

            /** NetworkPresenceRequest revoke. */
            public revoke: boolean;

            /**
             * Creates a new NetworkPresenceRequest instance using the specified properties.
             * @param [properties] Properties to set
             * @returns NetworkPresenceRequest instance
             */
            public static create(properties?: muster.v1.INetworkPresenceRequest): muster.v1.NetworkPresenceRequest;

            /**
             * Encodes the specified NetworkPresenceRequest message. Does not implicitly {@link muster.v1.NetworkPresenceRequest.verify|verify} messages.
             * @param message NetworkPresenceRequest message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.INetworkPresenceRequest, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a NetworkPresenceRequest message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns NetworkPresenceRequest
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.NetworkPresenceRequest;

            /**
             * Gets the default type url for NetworkPresenceRequest
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a PresenceEnrollment. */
        interface IPresenceEnrollment {

            /** PresenceEnrollment binding */
            binding?: (Uint8Array|null);

            /** PresenceEnrollment rosterEntries */
            rosterEntries?: (Uint8Array[]|null);
        }

        /** Represents a PresenceEnrollment. */
        class PresenceEnrollment implements IPresenceEnrollment {

            /**
             * Constructs a new PresenceEnrollment.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.IPresenceEnrollment);

            /** PresenceEnrollment binding. */
            public binding: Uint8Array;

            /** PresenceEnrollment rosterEntries. */
            public rosterEntries: Uint8Array[];

            /**
             * Creates a new PresenceEnrollment instance using the specified properties.
             * @param [properties] Properties to set
             * @returns PresenceEnrollment instance
             */
            public static create(properties?: muster.v1.IPresenceEnrollment): muster.v1.PresenceEnrollment;

            /**
             * Encodes the specified PresenceEnrollment message. Does not implicitly {@link muster.v1.PresenceEnrollment.verify|verify} messages.
             * @param message PresenceEnrollment message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.IPresenceEnrollment, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a PresenceEnrollment message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns PresenceEnrollment
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.PresenceEnrollment;

            /**
             * Gets the default type url for PresenceEnrollment
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a DeviceInstallPresenceRequest. */
        interface IDeviceInstallPresenceRequest {

            /** DeviceInstallPresenceRequest enrollment */
            enrollment?: (muster.v1.IPresenceEnrollment|null);
        }

        /** Represents a DeviceInstallPresenceRequest. */
        class DeviceInstallPresenceRequest implements IDeviceInstallPresenceRequest {

            /**
             * Constructs a new DeviceInstallPresenceRequest.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.IDeviceInstallPresenceRequest);

            /** DeviceInstallPresenceRequest enrollment. */
            public enrollment?: (muster.v1.IPresenceEnrollment|null);

            /**
             * Creates a new DeviceInstallPresenceRequest instance using the specified properties.
             * @param [properties] Properties to set
             * @returns DeviceInstallPresenceRequest instance
             */
            public static create(properties?: muster.v1.IDeviceInstallPresenceRequest): muster.v1.DeviceInstallPresenceRequest;

            /**
             * Encodes the specified DeviceInstallPresenceRequest message. Does not implicitly {@link muster.v1.DeviceInstallPresenceRequest.verify|verify} messages.
             * @param message DeviceInstallPresenceRequest message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.IDeviceInstallPresenceRequest, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a DeviceInstallPresenceRequest message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns DeviceInstallPresenceRequest
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.DeviceInstallPresenceRequest;

            /**
             * Gets the default type url for DeviceInstallPresenceRequest
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a PresenceInfo. */
        interface IPresenceInfo {

            /** PresenceInfo publicKey */
            publicKey?: (Uint8Array|null);

            /** PresenceInfo enrollment */
            enrollment?: (muster.v1.IPresenceEnrollment|null);

            /** PresenceInfo enrolled */
            enrolled?: (boolean|null);
        }

        /** Represents a PresenceInfo. */
        class PresenceInfo implements IPresenceInfo {

            /**
             * Constructs a new PresenceInfo.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.IPresenceInfo);

            /** PresenceInfo publicKey. */
            public publicKey: Uint8Array;

            /** PresenceInfo enrollment. */
            public enrollment?: (muster.v1.IPresenceEnrollment|null);

            /** PresenceInfo enrolled. */
            public enrolled: boolean;

            /**
             * Creates a new PresenceInfo instance using the specified properties.
             * @param [properties] Properties to set
             * @returns PresenceInfo instance
             */
            public static create(properties?: muster.v1.IPresenceInfo): muster.v1.PresenceInfo;

            /**
             * Encodes the specified PresenceInfo message. Does not implicitly {@link muster.v1.PresenceInfo.verify|verify} messages.
             * @param message PresenceInfo message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.IPresenceInfo, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a PresenceInfo message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns PresenceInfo
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.PresenceInfo;

            /**
             * Gets the default type url for PresenceInfo
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a MulticaRequest. */
        interface IMulticaRequest {

            /** MulticaRequest action */
            action?: (muster.v1.MulticaRequest.Action|null);

            /** MulticaRequest dataDir */
            dataDir?: (string|null);

            /** MulticaRequest installerUrl */
            installerUrl?: (string|null);

            /** MulticaRequest serverUrl */
            serverUrl?: (string|null);

            /** MulticaRequest appUrl */
            appUrl?: (string|null);

            /** MulticaRequest keepAlive */
            keepAlive?: (boolean|null);

            /** MulticaRequest containerName */
            containerName?: (string|null);
        }

        /** Represents a MulticaRequest. */
        class MulticaRequest implements IMulticaRequest {

            /**
             * Constructs a new MulticaRequest.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.IMulticaRequest);

            /** MulticaRequest action. */
            public action: muster.v1.MulticaRequest.Action;

            /** MulticaRequest dataDir. */
            public dataDir: string;

            /** MulticaRequest installerUrl. */
            public installerUrl: string;

            /** MulticaRequest serverUrl. */
            public serverUrl: string;

            /** MulticaRequest appUrl. */
            public appUrl: string;

            /** MulticaRequest keepAlive. */
            public keepAlive: boolean;

            /** MulticaRequest containerName. */
            public containerName: string;

            /**
             * Creates a new MulticaRequest instance using the specified properties.
             * @param [properties] Properties to set
             * @returns MulticaRequest instance
             */
            public static create(properties?: muster.v1.IMulticaRequest): muster.v1.MulticaRequest;

            /**
             * Encodes the specified MulticaRequest message. Does not implicitly {@link muster.v1.MulticaRequest.verify|verify} messages.
             * @param message MulticaRequest message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.IMulticaRequest, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a MulticaRequest message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns MulticaRequest
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.MulticaRequest;

            /**
             * Gets the default type url for MulticaRequest
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        namespace MulticaRequest {

            /** Action enum. */
            enum Action {
                CHECK = 0,
                SAVE = 1,
                INSTALL = 2,
                PROGRESS = 3
            }
        }

        /** Properties of a MulticaStatus. */
        interface IMulticaStatus {

            /** MulticaStatus supported */
            supported?: (boolean|null);

            /** MulticaStatus installed */
            installed?: (boolean|null);

            /** MulticaStatus running */
            running?: (boolean|null);

            /** MulticaStatus dataDir */
            dataDir?: (string|null);

            /** MulticaStatus installerUrl */
            installerUrl?: (string|null);

            /** MulticaStatus serverUrl */
            serverUrl?: (string|null);

            /** MulticaStatus appUrl */
            appUrl?: (string|null);

            /** MulticaStatus keepAlive */
            keepAlive?: (boolean|null);

            /** MulticaStatus containerName */
            containerName?: (string|null);

            /** MulticaStatus message */
            message?: (string|null);

            /** MulticaStatus loginCommand */
            loginCommand?: (string|null);

            /** MulticaStatus busy */
            busy?: (boolean|null);

            /** MulticaStatus installation */
            installation?: (muster.v1.IMulticaInstallation|null);

            /** MulticaStatus migrationRequired */
            migrationRequired?: (boolean|null);

            /** MulticaStatus version */
            version?: (string|null);
        }

        /** Represents a MulticaStatus. */
        class MulticaStatus implements IMulticaStatus {

            /**
             * Constructs a new MulticaStatus.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.IMulticaStatus);

            /** MulticaStatus supported. */
            public supported: boolean;

            /** MulticaStatus installed. */
            public installed: boolean;

            /** MulticaStatus running. */
            public running: boolean;

            /** MulticaStatus dataDir. */
            public dataDir: string;

            /** MulticaStatus installerUrl. */
            public installerUrl: string;

            /** MulticaStatus serverUrl. */
            public serverUrl: string;

            /** MulticaStatus appUrl. */
            public appUrl: string;

            /** MulticaStatus keepAlive. */
            public keepAlive: boolean;

            /** MulticaStatus containerName. */
            public containerName: string;

            /** MulticaStatus message. */
            public message: string;

            /** MulticaStatus loginCommand. */
            public loginCommand: string;

            /** MulticaStatus busy. */
            public busy: boolean;

            /** MulticaStatus installation. */
            public installation?: (muster.v1.IMulticaInstallation|null);

            /** MulticaStatus migrationRequired. */
            public migrationRequired: boolean;

            /** MulticaStatus version. */
            public version: string;

            /**
             * Creates a new MulticaStatus instance using the specified properties.
             * @param [properties] Properties to set
             * @returns MulticaStatus instance
             */
            public static create(properties?: muster.v1.IMulticaStatus): muster.v1.MulticaStatus;

            /**
             * Encodes the specified MulticaStatus message. Does not implicitly {@link muster.v1.MulticaStatus.verify|verify} messages.
             * @param message MulticaStatus message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.IMulticaStatus, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a MulticaStatus message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns MulticaStatus
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.MulticaStatus;

            /**
             * Gets the default type url for MulticaStatus
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a MulticaInstallation. */
        interface IMulticaInstallation {

            /** MulticaInstallation busy */
            busy?: (boolean|null);

            /** MulticaInstallation finished */
            finished?: (boolean|null);

            /** MulticaInstallation succeeded */
            succeeded?: (boolean|null);

            /** MulticaInstallation stage */
            stage?: (string|null);

            /** MulticaInstallation log */
            log?: (string|null);
        }

        /** Represents a MulticaInstallation. */
        class MulticaInstallation implements IMulticaInstallation {

            /**
             * Constructs a new MulticaInstallation.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.IMulticaInstallation);

            /** MulticaInstallation busy. */
            public busy: boolean;

            /** MulticaInstallation finished. */
            public finished: boolean;

            /** MulticaInstallation succeeded. */
            public succeeded: boolean;

            /** MulticaInstallation stage. */
            public stage: string;

            /** MulticaInstallation log. */
            public log: string;

            /**
             * Creates a new MulticaInstallation instance using the specified properties.
             * @param [properties] Properties to set
             * @returns MulticaInstallation instance
             */
            public static create(properties?: muster.v1.IMulticaInstallation): muster.v1.MulticaInstallation;

            /**
             * Encodes the specified MulticaInstallation message. Does not implicitly {@link muster.v1.MulticaInstallation.verify|verify} messages.
             * @param message MulticaInstallation message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.IMulticaInstallation, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a MulticaInstallation message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns MulticaInstallation
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.MulticaInstallation;

            /**
             * Gets the default type url for MulticaInstallation
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a MulticaBrowseRequest. */
        interface IMulticaBrowseRequest {

            /** MulticaBrowseRequest workspaceId */
            workspaceId?: (string|null);

            /** MulticaBrowseRequest offset */
            offset?: (number|null);

            /** MulticaBrowseRequest issueId */
            issueId?: (string|null);

            /** MulticaBrowseRequest agentId */
            agentId?: (string|null);
        }

        /** Represents a MulticaBrowseRequest. */
        class MulticaBrowseRequest implements IMulticaBrowseRequest {

            /**
             * Constructs a new MulticaBrowseRequest.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.IMulticaBrowseRequest);

            /** MulticaBrowseRequest workspaceId. */
            public workspaceId: string;

            /** MulticaBrowseRequest offset. */
            public offset: number;

            /** MulticaBrowseRequest issueId. */
            public issueId: string;

            /** MulticaBrowseRequest agentId. */
            public agentId: string;

            /**
             * Creates a new MulticaBrowseRequest instance using the specified properties.
             * @param [properties] Properties to set
             * @returns MulticaBrowseRequest instance
             */
            public static create(properties?: muster.v1.IMulticaBrowseRequest): muster.v1.MulticaBrowseRequest;

            /**
             * Encodes the specified MulticaBrowseRequest message. Does not implicitly {@link muster.v1.MulticaBrowseRequest.verify|verify} messages.
             * @param message MulticaBrowseRequest message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.IMulticaBrowseRequest, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a MulticaBrowseRequest message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns MulticaBrowseRequest
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.MulticaBrowseRequest;

            /**
             * Gets the default type url for MulticaBrowseRequest
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a MulticaWorkspace. */
        interface IMulticaWorkspace {

            /** MulticaWorkspace id */
            id?: (string|null);

            /** MulticaWorkspace name */
            name?: (string|null);
        }

        /** Represents a MulticaWorkspace. */
        class MulticaWorkspace implements IMulticaWorkspace {

            /**
             * Constructs a new MulticaWorkspace.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.IMulticaWorkspace);

            /** MulticaWorkspace id. */
            public id: string;

            /** MulticaWorkspace name. */
            public name: string;

            /**
             * Creates a new MulticaWorkspace instance using the specified properties.
             * @param [properties] Properties to set
             * @returns MulticaWorkspace instance
             */
            public static create(properties?: muster.v1.IMulticaWorkspace): muster.v1.MulticaWorkspace;

            /**
             * Encodes the specified MulticaWorkspace message. Does not implicitly {@link muster.v1.MulticaWorkspace.verify|verify} messages.
             * @param message MulticaWorkspace message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.IMulticaWorkspace, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a MulticaWorkspace message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns MulticaWorkspace
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.MulticaWorkspace;

            /**
             * Gets the default type url for MulticaWorkspace
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a MulticaIssue. */
        interface IMulticaIssue {

            /** MulticaIssue id */
            id?: (string|null);

            /** MulticaIssue identifier */
            identifier?: (string|null);

            /** MulticaIssue title */
            title?: (string|null);

            /** MulticaIssue description */
            description?: (string|null);

            /** MulticaIssue status */
            status?: (string|null);

            /** MulticaIssue priority */
            priority?: (string|null);

            /** MulticaIssue assigneeId */
            assigneeId?: (string|null);

            /** MulticaIssue assigneeType */
            assigneeType?: (string|null);

            /** MulticaIssue updatedAt */
            updatedAt?: (string|null);
        }

        /** Represents a MulticaIssue. */
        class MulticaIssue implements IMulticaIssue {

            /**
             * Constructs a new MulticaIssue.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.IMulticaIssue);

            /** MulticaIssue id. */
            public id: string;

            /** MulticaIssue identifier. */
            public identifier: string;

            /** MulticaIssue title. */
            public title: string;

            /** MulticaIssue description. */
            public description: string;

            /** MulticaIssue status. */
            public status: string;

            /** MulticaIssue priority. */
            public priority: string;

            /** MulticaIssue assigneeId. */
            public assigneeId: string;

            /** MulticaIssue assigneeType. */
            public assigneeType: string;

            /** MulticaIssue updatedAt. */
            public updatedAt: string;

            /**
             * Creates a new MulticaIssue instance using the specified properties.
             * @param [properties] Properties to set
             * @returns MulticaIssue instance
             */
            public static create(properties?: muster.v1.IMulticaIssue): muster.v1.MulticaIssue;

            /**
             * Encodes the specified MulticaIssue message. Does not implicitly {@link muster.v1.MulticaIssue.verify|verify} messages.
             * @param message MulticaIssue message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.IMulticaIssue, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a MulticaIssue message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns MulticaIssue
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.MulticaIssue;

            /**
             * Gets the default type url for MulticaIssue
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a MulticaAgent. */
        interface IMulticaAgent {

            /** MulticaAgent id */
            id?: (string|null);

            /** MulticaAgent name */
            name?: (string|null);

            /** MulticaAgent description */
            description?: (string|null);

            /** MulticaAgent status */
            status?: (string|null);

            /** MulticaAgent runtimeId */
            runtimeId?: (string|null);

            /** MulticaAgent model */
            model?: (string|null);
        }

        /** Represents a MulticaAgent. */
        class MulticaAgent implements IMulticaAgent {

            /**
             * Constructs a new MulticaAgent.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.IMulticaAgent);

            /** MulticaAgent id. */
            public id: string;

            /** MulticaAgent name. */
            public name: string;

            /** MulticaAgent description. */
            public description: string;

            /** MulticaAgent status. */
            public status: string;

            /** MulticaAgent runtimeId. */
            public runtimeId: string;

            /** MulticaAgent model. */
            public model: string;

            /**
             * Creates a new MulticaAgent instance using the specified properties.
             * @param [properties] Properties to set
             * @returns MulticaAgent instance
             */
            public static create(properties?: muster.v1.IMulticaAgent): muster.v1.MulticaAgent;

            /**
             * Encodes the specified MulticaAgent message. Does not implicitly {@link muster.v1.MulticaAgent.verify|verify} messages.
             * @param message MulticaAgent message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.IMulticaAgent, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a MulticaAgent message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns MulticaAgent
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.MulticaAgent;

            /**
             * Gets the default type url for MulticaAgent
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a MulticaRuntime. */
        interface IMulticaRuntime {

            /** MulticaRuntime id */
            id?: (string|null);

            /** MulticaRuntime name */
            name?: (string|null);

            /** MulticaRuntime provider */
            provider?: (string|null);

            /** MulticaRuntime status */
            status?: (string|null);

            /** MulticaRuntime lastSeenAt */
            lastSeenAt?: (string|null);

            /** MulticaRuntime deviceInfo */
            deviceInfo?: (string|null);
        }

        /** Represents a MulticaRuntime. */
        class MulticaRuntime implements IMulticaRuntime {

            /**
             * Constructs a new MulticaRuntime.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.IMulticaRuntime);

            /** MulticaRuntime id. */
            public id: string;

            /** MulticaRuntime name. */
            public name: string;

            /** MulticaRuntime provider. */
            public provider: string;

            /** MulticaRuntime status. */
            public status: string;

            /** MulticaRuntime lastSeenAt. */
            public lastSeenAt: string;

            /** MulticaRuntime deviceInfo. */
            public deviceInfo: string;

            /**
             * Creates a new MulticaRuntime instance using the specified properties.
             * @param [properties] Properties to set
             * @returns MulticaRuntime instance
             */
            public static create(properties?: muster.v1.IMulticaRuntime): muster.v1.MulticaRuntime;

            /**
             * Encodes the specified MulticaRuntime message. Does not implicitly {@link muster.v1.MulticaRuntime.verify|verify} messages.
             * @param message MulticaRuntime message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.IMulticaRuntime, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a MulticaRuntime message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns MulticaRuntime
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.MulticaRuntime;

            /**
             * Gets the default type url for MulticaRuntime
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a MulticaPage. */
        interface IMulticaPage {

            /** MulticaPage workspaces */
            workspaces?: (muster.v1.IMulticaWorkspace[]|null);

            /** MulticaPage workspaceId */
            workspaceId?: (string|null);

            /** MulticaPage issues */
            issues?: (muster.v1.IMulticaIssue[]|null);

            /** MulticaPage agents */
            agents?: (muster.v1.IMulticaAgent[]|null);

            /** MulticaPage runtimes */
            runtimes?: (muster.v1.IMulticaRuntime[]|null);

            /** MulticaPage offset */
            offset?: (number|null);

            /** MulticaPage hasMore */
            hasMore?: (boolean|null);

            /** MulticaPage issue */
            issue?: (muster.v1.IMulticaIssue|null);

            /** MulticaPage agent */
            agent?: (muster.v1.IMulticaAgent|null);

            /** MulticaPage busy */
            busy?: (boolean|null);

            /** MulticaPage error */
            error?: (string|null);

            /** MulticaPage nextOffset */
            nextOffset?: (number|null);

            /** MulticaPage previousOffset */
            previousOffset?: (number|null);
        }

        /** Represents a MulticaPage. */
        class MulticaPage implements IMulticaPage {

            /**
             * Constructs a new MulticaPage.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.IMulticaPage);

            /** MulticaPage workspaces. */
            public workspaces: muster.v1.IMulticaWorkspace[];

            /** MulticaPage workspaceId. */
            public workspaceId: string;

            /** MulticaPage issues. */
            public issues: muster.v1.IMulticaIssue[];

            /** MulticaPage agents. */
            public agents: muster.v1.IMulticaAgent[];

            /** MulticaPage runtimes. */
            public runtimes: muster.v1.IMulticaRuntime[];

            /** MulticaPage offset. */
            public offset: number;

            /** MulticaPage hasMore. */
            public hasMore: boolean;

            /** MulticaPage issue. */
            public issue?: (muster.v1.IMulticaIssue|null);

            /** MulticaPage agent. */
            public agent?: (muster.v1.IMulticaAgent|null);

            /** MulticaPage busy. */
            public busy: boolean;

            /** MulticaPage error. */
            public error: string;

            /** MulticaPage nextOffset. */
            public nextOffset: number;

            /** MulticaPage previousOffset. */
            public previousOffset: number;

            /**
             * Creates a new MulticaPage instance using the specified properties.
             * @param [properties] Properties to set
             * @returns MulticaPage instance
             */
            public static create(properties?: muster.v1.IMulticaPage): muster.v1.MulticaPage;

            /**
             * Encodes the specified MulticaPage message. Does not implicitly {@link muster.v1.MulticaPage.verify|verify} messages.
             * @param message MulticaPage message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.IMulticaPage, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a MulticaPage message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns MulticaPage
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.MulticaPage;

            /**
             * Gets the default type url for MulticaPage
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of an AgentNativeCreateRequest. */
        interface IAgentNativeCreateRequest {

            /** AgentNativeCreateRequest opId */
            opId?: (string|null);

            /** AgentNativeCreateRequest provider */
            provider?: (string|null);

            /** AgentNativeCreateRequest directory */
            directory?: (string|null);

            /** AgentNativeCreateRequest prompt */
            prompt?: (string|null);
        }

        /** Represents an AgentNativeCreateRequest. */
        class AgentNativeCreateRequest implements IAgentNativeCreateRequest {

            /**
             * Constructs a new AgentNativeCreateRequest.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.v1.IAgentNativeCreateRequest);

            /** AgentNativeCreateRequest opId. */
            public opId: string;

            /** AgentNativeCreateRequest provider. */
            public provider: string;

            /** AgentNativeCreateRequest directory. */
            public directory: string;

            /** AgentNativeCreateRequest prompt. */
            public prompt: string;

            /**
             * Creates a new AgentNativeCreateRequest instance using the specified properties.
             * @param [properties] Properties to set
             * @returns AgentNativeCreateRequest instance
             */
            public static create(properties?: muster.v1.IAgentNativeCreateRequest): muster.v1.AgentNativeCreateRequest;

            /**
             * Encodes the specified AgentNativeCreateRequest message. Does not implicitly {@link muster.v1.AgentNativeCreateRequest.verify|verify} messages.
             * @param message AgentNativeCreateRequest message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.v1.IAgentNativeCreateRequest, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes an AgentNativeCreateRequest message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns AgentNativeCreateRequest
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.v1.AgentNativeCreateRequest;

            /**
             * Gets the default type url for AgentNativeCreateRequest
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }
    }

    /** Namespace app. */
    namespace app {

        /** Properties of an Intent. */
        interface IIntent {

            /** Intent refresh */
            refresh?: (muster.app.IRefresh|null);

            /** Intent navigate */
            navigate?: (muster.app.INavigate|null);

            /** Intent dismiss */
            dismiss?: (muster.app.IDismiss|null);

            /** Intent createNetwork */
            createNetwork?: (muster.app.ICreateNetwork|null);

            /** Intent showInvite */
            showInvite?: (muster.app.IShowInvite|null);

            /** Intent joinNetwork */
            joinNetwork?: (muster.app.IJoinNetwork|null);

            /** Intent decideJoin */
            decideJoin?: (muster.app.IDecideJoin|null);

            /** Intent removeDevice */
            removeDevice?: (muster.app.IRemoveDevice|null);

            /** Intent setScopes */
            setScopes?: (muster.app.ISetScopes|null);

            /** Intent setPolicy */
            setPolicy?: (muster.app.ISetPolicy|null);

            /** Intent selectDevice */
            selectDevice?: (muster.app.ISelectDevice|null);

            /** Intent setRelay */
            setRelay?: (muster.app.ISetRelay|null);

            /** Intent requestAccessibility */
            requestAccessibility?: (muster.app.IRequestAccessibility|null);

            /** Intent openBrowser */
            openBrowser?: (muster.app.IOpenBrowser|null);

            /** Intent browserSetup */
            browserSetup?: (muster.app.IBrowserSetup|null);

            /** Intent browserRun */
            browserRun?: (muster.app.IBrowserRun|null);

            /** Intent setCover */
            setCover?: (muster.app.ISetCover|null);

            /** Intent cancelLockedUse */
            cancelLockedUse?: (muster.app.ICancelLockedUse|null);

            /** Intent setDeviceLockedUse */
            setDeviceLockedUse?: (muster.app.ISetDeviceLockedUse|null);

            /** Intent desktopSetupAction */
            desktopSetupAction?: (muster.app.IDesktopSetupAction|null);

            /** Intent desktopUpdateAction */
            desktopUpdateAction?: (muster.app.IDesktopUpdateAction|null);

            /** Intent multica */
            multica?: (muster.v1.IMulticaRequest|null);

            /** Intent multicaBrowse */
            multicaBrowse?: (muster.v1.IMulticaBrowseRequest|null);

            /** Intent openAgents */
            openAgents?: (muster.app.IOpenAgents|null);

            /** Intent orcaCreate */
            orcaCreate?: (muster.app.IOrcaCreate|null);

            /** Intent openAgent */
            openAgent?: (muster.app.IOpenAgent|null);

            /** Intent agentSend */
            agentSend?: (muster.app.IAgentSend|null);

            /** Intent resolveAgentSend */
            resolveAgentSend?: (muster.app.IResolveAgentSend|null);

            /** Intent agentRespond */
            agentRespond?: (muster.app.IAgentRespond|null);

            /** Intent agentInterrupt */
            agentInterrupt?: (muster.app.IAgentInterrupt|null);

            /** Intent agentLoadEarlier */
            agentLoadEarlier?: (muster.app.IAgentLoadEarlier|null);

            /** Intent openDesktopChats */
            openDesktopChats?: (muster.app.IOpenDesktopChats|null);

            /** Intent desktopChatOpen */
            desktopChatOpen?: (muster.app.IDesktopChatOpen|null);

            /** Intent chatOptionsLoad */
            chatOptionsLoad?: (muster.app.IChatOptionsLoad|null);

            /** Intent chatSetOptions */
            chatSetOptions?: (muster.app.IChatSetOptions|null);

            /** Intent composerAction */
            composerAction?: (muster.app.IComposerAction|null);

            /** Intent uploadDiagnostics */
            uploadDiagnostics?: (muster.app.IUploadDiagnostics|null);

            /** Intent composerComplete */
            composerComplete?: (muster.app.IComposerComplete|null);

            /** Intent openTerminals */
            openTerminals?: (muster.app.IOpenTerminals|null);

            /** Intent openTerminal */
            openTerminal?: (muster.app.IOpenTerminal|null);

            /** Intent terminalKey */
            terminalKey?: (muster.app.ITerminalKey|null);

            /** Intent terminalText */
            terminalText?: (muster.app.ITerminalText|null);

            /** Intent terminalCreate */
            terminalCreate?: (muster.app.ITerminalCreate|null);

            /** Intent terminalResize */
            terminalResize?: (muster.app.ITerminalResize|null);

            /** Intent openSsh */
            openSsh?: (muster.app.IOpenSsh|null);

            /** Intent closeTerminal */
            closeTerminal?: (muster.app.ICloseTerminal|null);

            /** Intent terminalPaste */
            terminalPaste?: (muster.app.ITerminalPaste|null);

            /** Intent sessionTerminalAction */
            sessionTerminalAction?: (muster.app.ISessionTerminalAction|null);

            /** Intent codeAction */
            codeAction?: (muster.app.ICodeAction|null);

            /** Intent openFiles */
            openFiles?: (muster.app.IOpenFiles|null);

            /** Intent openFile */
            openFile?: (muster.app.IOpenFile|null);

            /** Intent openGit */
            openGit?: (muster.app.IOpenGit|null);

            /** Intent openChange */
            openChange?: (muster.app.IOpenChange|null);

            /** Intent openFileLink */
            openFileLink?: (muster.app.IOpenFileLink|null);

            /** Intent setPushToken */
            setPushToken?: (muster.app.ISetPushToken|null);
        }

        /** Represents an Intent. */
        class Intent implements IIntent {

            /**
             * Constructs a new Intent.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.app.IIntent);

            /** Intent refresh. */
            public refresh?: (muster.app.IRefresh|null);

            /** Intent navigate. */
            public navigate?: (muster.app.INavigate|null);

            /** Intent dismiss. */
            public dismiss?: (muster.app.IDismiss|null);

            /** Intent createNetwork. */
            public createNetwork?: (muster.app.ICreateNetwork|null);

            /** Intent showInvite. */
            public showInvite?: (muster.app.IShowInvite|null);

            /** Intent joinNetwork. */
            public joinNetwork?: (muster.app.IJoinNetwork|null);

            /** Intent decideJoin. */
            public decideJoin?: (muster.app.IDecideJoin|null);

            /** Intent removeDevice. */
            public removeDevice?: (muster.app.IRemoveDevice|null);

            /** Intent setScopes. */
            public setScopes?: (muster.app.ISetScopes|null);

            /** Intent setPolicy. */
            public setPolicy?: (muster.app.ISetPolicy|null);

            /** Intent selectDevice. */
            public selectDevice?: (muster.app.ISelectDevice|null);

            /** Intent setRelay. */
            public setRelay?: (muster.app.ISetRelay|null);

            /** Intent requestAccessibility. */
            public requestAccessibility?: (muster.app.IRequestAccessibility|null);

            /** Intent openBrowser. */
            public openBrowser?: (muster.app.IOpenBrowser|null);

            /** Intent browserSetup. */
            public browserSetup?: (muster.app.IBrowserSetup|null);

            /** Intent browserRun. */
            public browserRun?: (muster.app.IBrowserRun|null);

            /** Intent setCover. */
            public setCover?: (muster.app.ISetCover|null);

            /** Intent cancelLockedUse. */
            public cancelLockedUse?: (muster.app.ICancelLockedUse|null);

            /** Intent setDeviceLockedUse. */
            public setDeviceLockedUse?: (muster.app.ISetDeviceLockedUse|null);

            /** Intent desktopSetupAction. */
            public desktopSetupAction?: (muster.app.IDesktopSetupAction|null);

            /** Intent desktopUpdateAction. */
            public desktopUpdateAction?: (muster.app.IDesktopUpdateAction|null);

            /** Intent multica. */
            public multica?: (muster.v1.IMulticaRequest|null);

            /** Intent multicaBrowse. */
            public multicaBrowse?: (muster.v1.IMulticaBrowseRequest|null);

            /** Intent openAgents. */
            public openAgents?: (muster.app.IOpenAgents|null);

            /** Intent orcaCreate. */
            public orcaCreate?: (muster.app.IOrcaCreate|null);

            /** Intent openAgent. */
            public openAgent?: (muster.app.IOpenAgent|null);

            /** Intent agentSend. */
            public agentSend?: (muster.app.IAgentSend|null);

            /** Intent resolveAgentSend. */
            public resolveAgentSend?: (muster.app.IResolveAgentSend|null);

            /** Intent agentRespond. */
            public agentRespond?: (muster.app.IAgentRespond|null);

            /** Intent agentInterrupt. */
            public agentInterrupt?: (muster.app.IAgentInterrupt|null);

            /** Intent agentLoadEarlier. */
            public agentLoadEarlier?: (muster.app.IAgentLoadEarlier|null);

            /** Intent openDesktopChats. */
            public openDesktopChats?: (muster.app.IOpenDesktopChats|null);

            /** Intent desktopChatOpen. */
            public desktopChatOpen?: (muster.app.IDesktopChatOpen|null);

            /** Intent chatOptionsLoad. */
            public chatOptionsLoad?: (muster.app.IChatOptionsLoad|null);

            /** Intent chatSetOptions. */
            public chatSetOptions?: (muster.app.IChatSetOptions|null);

            /** Intent composerAction. */
            public composerAction?: (muster.app.IComposerAction|null);

            /** Intent uploadDiagnostics. */
            public uploadDiagnostics?: (muster.app.IUploadDiagnostics|null);

            /** Intent composerComplete. */
            public composerComplete?: (muster.app.IComposerComplete|null);

            /** Intent openTerminals. */
            public openTerminals?: (muster.app.IOpenTerminals|null);

            /** Intent openTerminal. */
            public openTerminal?: (muster.app.IOpenTerminal|null);

            /** Intent terminalKey. */
            public terminalKey?: (muster.app.ITerminalKey|null);

            /** Intent terminalText. */
            public terminalText?: (muster.app.ITerminalText|null);

            /** Intent terminalCreate. */
            public terminalCreate?: (muster.app.ITerminalCreate|null);

            /** Intent terminalResize. */
            public terminalResize?: (muster.app.ITerminalResize|null);

            /** Intent openSsh. */
            public openSsh?: (muster.app.IOpenSsh|null);

            /** Intent closeTerminal. */
            public closeTerminal?: (muster.app.ICloseTerminal|null);

            /** Intent terminalPaste. */
            public terminalPaste?: (muster.app.ITerminalPaste|null);

            /** Intent sessionTerminalAction. */
            public sessionTerminalAction?: (muster.app.ISessionTerminalAction|null);

            /** Intent codeAction. */
            public codeAction?: (muster.app.ICodeAction|null);

            /** Intent openFiles. */
            public openFiles?: (muster.app.IOpenFiles|null);

            /** Intent openFile. */
            public openFile?: (muster.app.IOpenFile|null);

            /** Intent openGit. */
            public openGit?: (muster.app.IOpenGit|null);

            /** Intent openChange. */
            public openChange?: (muster.app.IOpenChange|null);

            /** Intent openFileLink. */
            public openFileLink?: (muster.app.IOpenFileLink|null);

            /** Intent setPushToken. */
            public setPushToken?: (muster.app.ISetPushToken|null);

            /** Intent it. */
            public it?: ("refresh"|"navigate"|"dismiss"|"createNetwork"|"showInvite"|"joinNetwork"|"decideJoin"|"removeDevice"|"setScopes"|"setPolicy"|"selectDevice"|"setRelay"|"requestAccessibility"|"openBrowser"|"browserSetup"|"browserRun"|"setCover"|"cancelLockedUse"|"setDeviceLockedUse"|"desktopSetupAction"|"desktopUpdateAction"|"multica"|"multicaBrowse"|"openAgents"|"orcaCreate"|"openAgent"|"agentSend"|"resolveAgentSend"|"agentRespond"|"agentInterrupt"|"agentLoadEarlier"|"openDesktopChats"|"desktopChatOpen"|"chatOptionsLoad"|"chatSetOptions"|"composerAction"|"uploadDiagnostics"|"composerComplete"|"openTerminals"|"openTerminal"|"terminalKey"|"terminalText"|"terminalCreate"|"terminalResize"|"openSsh"|"closeTerminal"|"terminalPaste"|"sessionTerminalAction"|"codeAction"|"openFiles"|"openFile"|"openGit"|"openChange"|"openFileLink"|"setPushToken");

            /**
             * Creates a new Intent instance using the specified properties.
             * @param [properties] Properties to set
             * @returns Intent instance
             */
            public static create(properties?: muster.app.IIntent): muster.app.Intent;

            /**
             * Encodes the specified Intent message. Does not implicitly {@link muster.app.Intent.verify|verify} messages.
             * @param message Intent message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.app.IIntent, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes an Intent message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns Intent
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.app.Intent;

            /**
             * Gets the default type url for Intent
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a DesktopUpdateAction. */
        interface IDesktopUpdateAction {

            /** DesktopUpdateAction action */
            action?: (muster.app.DesktopUpdateAction.Action|null);
        }

        /** Represents a DesktopUpdateAction. */
        class DesktopUpdateAction implements IDesktopUpdateAction {

            /**
             * Constructs a new DesktopUpdateAction.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.app.IDesktopUpdateAction);

            /** DesktopUpdateAction action. */
            public action: muster.app.DesktopUpdateAction.Action;

            /**
             * Creates a new DesktopUpdateAction instance using the specified properties.
             * @param [properties] Properties to set
             * @returns DesktopUpdateAction instance
             */
            public static create(properties?: muster.app.IDesktopUpdateAction): muster.app.DesktopUpdateAction;

            /**
             * Encodes the specified DesktopUpdateAction message. Does not implicitly {@link muster.app.DesktopUpdateAction.verify|verify} messages.
             * @param message DesktopUpdateAction message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.app.IDesktopUpdateAction, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a DesktopUpdateAction message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns DesktopUpdateAction
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.app.DesktopUpdateAction;

            /**
             * Gets the default type url for DesktopUpdateAction
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        namespace DesktopUpdateAction {

            /** Action enum. */
            enum Action {
                CHECK = 0,
                DOWNLOAD = 1,
                INSTALL = 2
            }
        }

        /** Properties of a DesktopUpdateView. */
        interface IDesktopUpdateView {

            /** DesktopUpdateView supported */
            supported?: (boolean|null);

            /** DesktopUpdateView status */
            status?: (string|null);

            /** DesktopUpdateView version */
            version?: (string|null);

            /** DesktopUpdateView error */
            error?: (string|null);

            /** DesktopUpdateView canCheck */
            canCheck?: (boolean|null);

            /** DesktopUpdateView canDownload */
            canDownload?: (boolean|null);

            /** DesktopUpdateView canInstall */
            canInstall?: (boolean|null);
        }

        /** Represents a DesktopUpdateView. */
        class DesktopUpdateView implements IDesktopUpdateView {

            /**
             * Constructs a new DesktopUpdateView.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.app.IDesktopUpdateView);

            /** DesktopUpdateView supported. */
            public supported: boolean;

            /** DesktopUpdateView status. */
            public status: string;

            /** DesktopUpdateView version. */
            public version: string;

            /** DesktopUpdateView error. */
            public error: string;

            /** DesktopUpdateView canCheck. */
            public canCheck: boolean;

            /** DesktopUpdateView canDownload. */
            public canDownload: boolean;

            /** DesktopUpdateView canInstall. */
            public canInstall: boolean;

            /**
             * Creates a new DesktopUpdateView instance using the specified properties.
             * @param [properties] Properties to set
             * @returns DesktopUpdateView instance
             */
            public static create(properties?: muster.app.IDesktopUpdateView): muster.app.DesktopUpdateView;

            /**
             * Encodes the specified DesktopUpdateView message. Does not implicitly {@link muster.app.DesktopUpdateView.verify|verify} messages.
             * @param message DesktopUpdateView message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.app.IDesktopUpdateView, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a DesktopUpdateView message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns DesktopUpdateView
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.app.DesktopUpdateView;

            /**
             * Gets the default type url for DesktopUpdateView
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a DesktopSetupAction. */
        interface IDesktopSetupAction {

            /** DesktopSetupAction action */
            action?: (muster.app.DesktopSetupAction.Action|null);
        }

        /** Represents a DesktopSetupAction. */
        class DesktopSetupAction implements IDesktopSetupAction {

            /**
             * Constructs a new DesktopSetupAction.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.app.IDesktopSetupAction);

            /** DesktopSetupAction action. */
            public action: muster.app.DesktopSetupAction.Action;

            /**
             * Creates a new DesktopSetupAction instance using the specified properties.
             * @param [properties] Properties to set
             * @returns DesktopSetupAction instance
             */
            public static create(properties?: muster.app.IDesktopSetupAction): muster.app.DesktopSetupAction;

            /**
             * Encodes the specified DesktopSetupAction message. Does not implicitly {@link muster.app.DesktopSetupAction.verify|verify} messages.
             * @param message DesktopSetupAction message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.app.IDesktopSetupAction, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a DesktopSetupAction message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns DesktopSetupAction
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.app.DesktopSetupAction;

            /**
             * Gets the default type url for DesktopSetupAction
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        namespace DesktopSetupAction {

            /** Action enum. */
            enum Action {
                SHOW = 0,
                DISMISS = 1,
                INSTALL = 2,
                ITERM = 3,
                CHECK = 4,
                RESTART = 5
            }
        }

        /** Properties of a SetDeviceLockedUse. */
        interface ISetDeviceLockedUse {

            /** SetDeviceLockedUse deviceId */
            deviceId?: (string|null);

            /** SetDeviceLockedUse on */
            on?: (boolean|null);
        }

        /** Represents a SetDeviceLockedUse. */
        class SetDeviceLockedUse implements ISetDeviceLockedUse {

            /**
             * Constructs a new SetDeviceLockedUse.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.app.ISetDeviceLockedUse);

            /** SetDeviceLockedUse deviceId. */
            public deviceId: string;

            /** SetDeviceLockedUse on. */
            public on: boolean;

            /**
             * Creates a new SetDeviceLockedUse instance using the specified properties.
             * @param [properties] Properties to set
             * @returns SetDeviceLockedUse instance
             */
            public static create(properties?: muster.app.ISetDeviceLockedUse): muster.app.SetDeviceLockedUse;

            /**
             * Encodes the specified SetDeviceLockedUse message. Does not implicitly {@link muster.app.SetDeviceLockedUse.verify|verify} messages.
             * @param message SetDeviceLockedUse message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.app.ISetDeviceLockedUse, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a SetDeviceLockedUse message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns SetDeviceLockedUse
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.app.SetDeviceLockedUse;

            /**
             * Gets the default type url for SetDeviceLockedUse
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a DeviceLockedUseView. */
        interface IDeviceLockedUseView {

            /** DeviceLockedUseView deviceId */
            deviceId?: (string|null);

            /** DeviceLockedUseView on */
            on?: (boolean|null);

            /** DeviceLockedUseView enabled */
            enabled?: (boolean|null);

            /** DeviceLockedUseView busy */
            busy?: (boolean|null);

            /** DeviceLockedUseView status */
            status?: (string|null);
        }

        /** Represents a DeviceLockedUseView. */
        class DeviceLockedUseView implements IDeviceLockedUseView {

            /**
             * Constructs a new DeviceLockedUseView.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.app.IDeviceLockedUseView);

            /** DeviceLockedUseView deviceId. */
            public deviceId: string;

            /** DeviceLockedUseView on. */
            public on: boolean;

            /** DeviceLockedUseView enabled. */
            public enabled: boolean;

            /** DeviceLockedUseView busy. */
            public busy: boolean;

            /** DeviceLockedUseView status. */
            public status: string;

            /**
             * Creates a new DeviceLockedUseView instance using the specified properties.
             * @param [properties] Properties to set
             * @returns DeviceLockedUseView instance
             */
            public static create(properties?: muster.app.IDeviceLockedUseView): muster.app.DeviceLockedUseView;

            /**
             * Encodes the specified DeviceLockedUseView message. Does not implicitly {@link muster.app.DeviceLockedUseView.verify|verify} messages.
             * @param message DeviceLockedUseView message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.app.IDeviceLockedUseView, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a DeviceLockedUseView message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns DeviceLockedUseView
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.app.DeviceLockedUseView;

            /**
             * Gets the default type url for DeviceLockedUseView
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a CancelLockedUse. */
        interface ICancelLockedUse {
        }

        /** Represents a CancelLockedUse. */
        class CancelLockedUse implements ICancelLockedUse {

            /**
             * Constructs a new CancelLockedUse.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.app.ICancelLockedUse);

            /**
             * Creates a new CancelLockedUse instance using the specified properties.
             * @param [properties] Properties to set
             * @returns CancelLockedUse instance
             */
            public static create(properties?: muster.app.ICancelLockedUse): muster.app.CancelLockedUse;

            /**
             * Encodes the specified CancelLockedUse message. Does not implicitly {@link muster.app.CancelLockedUse.verify|verify} messages.
             * @param message CancelLockedUse message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.app.ICancelLockedUse, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a CancelLockedUse message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns CancelLockedUse
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.app.CancelLockedUse;

            /**
             * Gets the default type url for CancelLockedUse
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a Refresh. */
        interface IRefresh {
        }

        /** Represents a Refresh. */
        class Refresh implements IRefresh {

            /**
             * Constructs a new Refresh.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.app.IRefresh);

            /**
             * Creates a new Refresh instance using the specified properties.
             * @param [properties] Properties to set
             * @returns Refresh instance
             */
            public static create(properties?: muster.app.IRefresh): muster.app.Refresh;

            /**
             * Encodes the specified Refresh message. Does not implicitly {@link muster.app.Refresh.verify|verify} messages.
             * @param message Refresh message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.app.IRefresh, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a Refresh message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns Refresh
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.app.Refresh;

            /**
             * Gets the default type url for Refresh
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a Navigate. */
        interface INavigate {

            /** Navigate screen */
            screen?: (muster.app.Screen|null);

            /** Navigate back */
            back?: (boolean|null);

            /** Navigate backCount */
            backCount?: (number|null);

            /** Navigate expectedPath */
            expectedPath?: (muster.app.Screen[]|null);
        }

        /** Represents a Navigate. */
        class Navigate implements INavigate {

            /**
             * Constructs a new Navigate.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.app.INavigate);

            /** Navigate screen. */
            public screen: muster.app.Screen;

            /** Navigate back. */
            public back: boolean;

            /** Navigate backCount. */
            public backCount: number;

            /** Navigate expectedPath. */
            public expectedPath: muster.app.Screen[];

            /**
             * Creates a new Navigate instance using the specified properties.
             * @param [properties] Properties to set
             * @returns Navigate instance
             */
            public static create(properties?: muster.app.INavigate): muster.app.Navigate;

            /**
             * Encodes the specified Navigate message. Does not implicitly {@link muster.app.Navigate.verify|verify} messages.
             * @param message Navigate message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.app.INavigate, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a Navigate message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns Navigate
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.app.Navigate;

            /**
             * Gets the default type url for Navigate
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a Dismiss. */
        interface IDismiss {

            /** Dismiss toastId */
            toastId?: (string|null);
        }

        /** Represents a Dismiss. */
        class Dismiss implements IDismiss {

            /**
             * Constructs a new Dismiss.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.app.IDismiss);

            /** Dismiss toastId. */
            public toastId: string;

            /**
             * Creates a new Dismiss instance using the specified properties.
             * @param [properties] Properties to set
             * @returns Dismiss instance
             */
            public static create(properties?: muster.app.IDismiss): muster.app.Dismiss;

            /**
             * Encodes the specified Dismiss message. Does not implicitly {@link muster.app.Dismiss.verify|verify} messages.
             * @param message Dismiss message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.app.IDismiss, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a Dismiss message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns Dismiss
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.app.Dismiss;

            /**
             * Gets the default type url for Dismiss
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a CreateNetwork. */
        interface ICreateNetwork {

            /** CreateNetwork name */
            name?: (string|null);

            /** CreateNetwork vnetCidr */
            vnetCidr?: (string|null);
        }

        /** Represents a CreateNetwork. */
        class CreateNetwork implements ICreateNetwork {

            /**
             * Constructs a new CreateNetwork.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.app.ICreateNetwork);

            /** CreateNetwork name. */
            public name: string;

            /** CreateNetwork vnetCidr. */
            public vnetCidr: string;

            /**
             * Creates a new CreateNetwork instance using the specified properties.
             * @param [properties] Properties to set
             * @returns CreateNetwork instance
             */
            public static create(properties?: muster.app.ICreateNetwork): muster.app.CreateNetwork;

            /**
             * Encodes the specified CreateNetwork message. Does not implicitly {@link muster.app.CreateNetwork.verify|verify} messages.
             * @param message CreateNetwork message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.app.ICreateNetwork, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a CreateNetwork message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns CreateNetwork
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.app.CreateNetwork;

            /**
             * Gets the default type url for CreateNetwork
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a ShowInvite. */
        interface IShowInvite {

            /** ShowInvite ttlSecs */
            ttlSecs?: (number|null);
        }

        /** Represents a ShowInvite. */
        class ShowInvite implements IShowInvite {

            /**
             * Constructs a new ShowInvite.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.app.IShowInvite);

            /** ShowInvite ttlSecs. */
            public ttlSecs: number;

            /**
             * Creates a new ShowInvite instance using the specified properties.
             * @param [properties] Properties to set
             * @returns ShowInvite instance
             */
            public static create(properties?: muster.app.IShowInvite): muster.app.ShowInvite;

            /**
             * Encodes the specified ShowInvite message. Does not implicitly {@link muster.app.ShowInvite.verify|verify} messages.
             * @param message ShowInvite message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.app.IShowInvite, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a ShowInvite message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns ShowInvite
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.app.ShowInvite;

            /**
             * Gets the default type url for ShowInvite
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a JoinNetwork. */
        interface IJoinNetwork {

            /** JoinNetwork inviteText */
            inviteText?: (string|null);

            /** JoinNetwork deviceName */
            deviceName?: (string|null);

            /** JoinNetwork deviceKind */
            deviceKind?: (string|null);
        }

        /** Represents a JoinNetwork. */
        class JoinNetwork implements IJoinNetwork {

            /**
             * Constructs a new JoinNetwork.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.app.IJoinNetwork);

            /** JoinNetwork inviteText. */
            public inviteText: string;

            /** JoinNetwork deviceName. */
            public deviceName: string;

            /** JoinNetwork deviceKind. */
            public deviceKind: string;

            /**
             * Creates a new JoinNetwork instance using the specified properties.
             * @param [properties] Properties to set
             * @returns JoinNetwork instance
             */
            public static create(properties?: muster.app.IJoinNetwork): muster.app.JoinNetwork;

            /**
             * Encodes the specified JoinNetwork message. Does not implicitly {@link muster.app.JoinNetwork.verify|verify} messages.
             * @param message JoinNetwork message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.app.IJoinNetwork, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a JoinNetwork message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns JoinNetwork
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.app.JoinNetwork;

            /**
             * Gets the default type url for JoinNetwork
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a DecideJoin. */
        interface IDecideJoin {

            /** DecideJoin deviceId */
            deviceId?: (string|null);

            /** DecideJoin approve */
            approve?: (boolean|null);

            /** DecideJoin scopes */
            scopes?: (string[]|null);

            /** DecideJoin reason */
            reason?: (string|null);
        }

        /** Represents a DecideJoin. */
        class DecideJoin implements IDecideJoin {

            /**
             * Constructs a new DecideJoin.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.app.IDecideJoin);

            /** DecideJoin deviceId. */
            public deviceId: string;

            /** DecideJoin approve. */
            public approve: boolean;

            /** DecideJoin scopes. */
            public scopes: string[];

            /** DecideJoin reason. */
            public reason: string;

            /**
             * Creates a new DecideJoin instance using the specified properties.
             * @param [properties] Properties to set
             * @returns DecideJoin instance
             */
            public static create(properties?: muster.app.IDecideJoin): muster.app.DecideJoin;

            /**
             * Encodes the specified DecideJoin message. Does not implicitly {@link muster.app.DecideJoin.verify|verify} messages.
             * @param message DecideJoin message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.app.IDecideJoin, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a DecideJoin message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns DecideJoin
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.app.DecideJoin;

            /**
             * Gets the default type url for DecideJoin
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a RemoveDevice. */
        interface IRemoveDevice {

            /** RemoveDevice deviceId */
            deviceId?: (string|null);
        }

        /** Represents a RemoveDevice. */
        class RemoveDevice implements IRemoveDevice {

            /**
             * Constructs a new RemoveDevice.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.app.IRemoveDevice);

            /** RemoveDevice deviceId. */
            public deviceId: string;

            /**
             * Creates a new RemoveDevice instance using the specified properties.
             * @param [properties] Properties to set
             * @returns RemoveDevice instance
             */
            public static create(properties?: muster.app.IRemoveDevice): muster.app.RemoveDevice;

            /**
             * Encodes the specified RemoveDevice message. Does not implicitly {@link muster.app.RemoveDevice.verify|verify} messages.
             * @param message RemoveDevice message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.app.IRemoveDevice, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a RemoveDevice message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns RemoveDevice
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.app.RemoveDevice;

            /**
             * Gets the default type url for RemoveDevice
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a SetScopes. */
        interface ISetScopes {

            /** SetScopes deviceId */
            deviceId?: (string|null);

            /** SetScopes scopes */
            scopes?: (string[]|null);

            /** SetScopes role */
            role?: (string|null);
        }

        /** Represents a SetScopes. */
        class SetScopes implements ISetScopes {

            /**
             * Constructs a new SetScopes.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.app.ISetScopes);

            /** SetScopes deviceId. */
            public deviceId: string;

            /** SetScopes scopes. */
            public scopes: string[];

            /** SetScopes role. */
            public role: string;

            /**
             * Creates a new SetScopes instance using the specified properties.
             * @param [properties] Properties to set
             * @returns SetScopes instance
             */
            public static create(properties?: muster.app.ISetScopes): muster.app.SetScopes;

            /**
             * Encodes the specified SetScopes message. Does not implicitly {@link muster.app.SetScopes.verify|verify} messages.
             * @param message SetScopes message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.app.ISetScopes, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a SetScopes message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns SetScopes
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.app.SetScopes;

            /**
             * Gets the default type url for SetScopes
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a SetRelay. */
        interface ISetRelay {

            /** SetRelay relay */
            relay?: (string|null);
        }

        /** Represents a SetRelay. */
        class SetRelay implements ISetRelay {

            /**
             * Constructs a new SetRelay.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.app.ISetRelay);

            /** SetRelay relay. */
            public relay: string;

            /**
             * Creates a new SetRelay instance using the specified properties.
             * @param [properties] Properties to set
             * @returns SetRelay instance
             */
            public static create(properties?: muster.app.ISetRelay): muster.app.SetRelay;

            /**
             * Encodes the specified SetRelay message. Does not implicitly {@link muster.app.SetRelay.verify|verify} messages.
             * @param message SetRelay message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.app.ISetRelay, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a SetRelay message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns SetRelay
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.app.SetRelay;

            /**
             * Gets the default type url for SetRelay
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a RequestAccessibility. */
        interface IRequestAccessibility {
        }

        /** Represents a RequestAccessibility. */
        class RequestAccessibility implements IRequestAccessibility {

            /**
             * Constructs a new RequestAccessibility.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.app.IRequestAccessibility);

            /**
             * Creates a new RequestAccessibility instance using the specified properties.
             * @param [properties] Properties to set
             * @returns RequestAccessibility instance
             */
            public static create(properties?: muster.app.IRequestAccessibility): muster.app.RequestAccessibility;

            /**
             * Encodes the specified RequestAccessibility message. Does not implicitly {@link muster.app.RequestAccessibility.verify|verify} messages.
             * @param message RequestAccessibility message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.app.IRequestAccessibility, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a RequestAccessibility message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns RequestAccessibility
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.app.RequestAccessibility;

            /**
             * Gets the default type url for RequestAccessibility
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a SetCover. */
        interface ISetCover {

            /** SetCover on */
            on?: (boolean|null);

            /** SetCover deviceId */
            deviceId?: (string|null);
        }

        /** Represents a SetCover. */
        class SetCover implements ISetCover {

            /**
             * Constructs a new SetCover.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.app.ISetCover);

            /** SetCover on. */
            public on: boolean;

            /** SetCover deviceId. */
            public deviceId: string;

            /**
             * Creates a new SetCover instance using the specified properties.
             * @param [properties] Properties to set
             * @returns SetCover instance
             */
            public static create(properties?: muster.app.ISetCover): muster.app.SetCover;

            /**
             * Encodes the specified SetCover message. Does not implicitly {@link muster.app.SetCover.verify|verify} messages.
             * @param message SetCover message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.app.ISetCover, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a SetCover message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns SetCover
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.app.SetCover;

            /**
             * Gets the default type url for SetCover
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a SetPolicy. */
        interface ISetPolicy {

            /** SetPolicy policy */
            policy?: (muster.v1.IPolicy|null);
        }

        /** Represents a SetPolicy. */
        class SetPolicy implements ISetPolicy {

            /**
             * Constructs a new SetPolicy.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.app.ISetPolicy);

            /** SetPolicy policy. */
            public policy?: (muster.v1.IPolicy|null);

            /**
             * Creates a new SetPolicy instance using the specified properties.
             * @param [properties] Properties to set
             * @returns SetPolicy instance
             */
            public static create(properties?: muster.app.ISetPolicy): muster.app.SetPolicy;

            /**
             * Encodes the specified SetPolicy message. Does not implicitly {@link muster.app.SetPolicy.verify|verify} messages.
             * @param message SetPolicy message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.app.ISetPolicy, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a SetPolicy message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns SetPolicy
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.app.SetPolicy;

            /**
             * Gets the default type url for SetPolicy
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a SelectDevice. */
        interface ISelectDevice {

            /** SelectDevice deviceId */
            deviceId?: (string|null);
        }

        /** Represents a SelectDevice. */
        class SelectDevice implements ISelectDevice {

            /**
             * Constructs a new SelectDevice.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.app.ISelectDevice);

            /** SelectDevice deviceId. */
            public deviceId: string;

            /**
             * Creates a new SelectDevice instance using the specified properties.
             * @param [properties] Properties to set
             * @returns SelectDevice instance
             */
            public static create(properties?: muster.app.ISelectDevice): muster.app.SelectDevice;

            /**
             * Encodes the specified SelectDevice message. Does not implicitly {@link muster.app.SelectDevice.verify|verify} messages.
             * @param message SelectDevice message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.app.ISelectDevice, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a SelectDevice message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns SelectDevice
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.app.SelectDevice;

            /**
             * Gets the default type url for SelectDevice
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of an OrcaCreate. */
        interface IOrcaCreate {

            /** OrcaCreate deviceId */
            deviceId?: (string|null);

            /** OrcaCreate directory */
            directory?: (string|null);

            /** OrcaCreate agent */
            agent?: (string|null);
        }

        /** Represents an OrcaCreate. */
        class OrcaCreate implements IOrcaCreate {

            /**
             * Constructs a new OrcaCreate.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.app.IOrcaCreate);

            /** OrcaCreate deviceId. */
            public deviceId: string;

            /** OrcaCreate directory. */
            public directory: string;

            /** OrcaCreate agent. */
            public agent: string;

            /**
             * Creates a new OrcaCreate instance using the specified properties.
             * @param [properties] Properties to set
             * @returns OrcaCreate instance
             */
            public static create(properties?: muster.app.IOrcaCreate): muster.app.OrcaCreate;

            /**
             * Encodes the specified OrcaCreate message. Does not implicitly {@link muster.app.OrcaCreate.verify|verify} messages.
             * @param message OrcaCreate message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.app.IOrcaCreate, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes an OrcaCreate message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns OrcaCreate
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.app.OrcaCreate;

            /**
             * Gets the default type url for OrcaCreate
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of an OpenAgents. */
        interface IOpenAgents {

            /** OpenAgents deviceId */
            deviceId?: (string|null);
        }

        /** Represents an OpenAgents. */
        class OpenAgents implements IOpenAgents {

            /**
             * Constructs a new OpenAgents.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.app.IOpenAgents);

            /** OpenAgents deviceId. */
            public deviceId: string;

            /**
             * Creates a new OpenAgents instance using the specified properties.
             * @param [properties] Properties to set
             * @returns OpenAgents instance
             */
            public static create(properties?: muster.app.IOpenAgents): muster.app.OpenAgents;

            /**
             * Encodes the specified OpenAgents message. Does not implicitly {@link muster.app.OpenAgents.verify|verify} messages.
             * @param message OpenAgents message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.app.IOpenAgents, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes an OpenAgents message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns OpenAgents
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.app.OpenAgents;

            /**
             * Gets the default type url for OpenAgents
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of an OpenAgent. */
        interface IOpenAgent {

            /** OpenAgent session */
            session?: (string|null);

            /** OpenAgent deviceId */
            deviceId?: (string|null);
        }

        /** Represents an OpenAgent. */
        class OpenAgent implements IOpenAgent {

            /**
             * Constructs a new OpenAgent.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.app.IOpenAgent);

            /** OpenAgent session. */
            public session: string;

            /** OpenAgent deviceId. */
            public deviceId: string;

            /**
             * Creates a new OpenAgent instance using the specified properties.
             * @param [properties] Properties to set
             * @returns OpenAgent instance
             */
            public static create(properties?: muster.app.IOpenAgent): muster.app.OpenAgent;

            /**
             * Encodes the specified OpenAgent message. Does not implicitly {@link muster.app.OpenAgent.verify|verify} messages.
             * @param message OpenAgent message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.app.IOpenAgent, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes an OpenAgent message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns OpenAgent
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.app.OpenAgent;

            /**
             * Gets the default type url for OpenAgent
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a ResolveAgentSend. */
        interface IResolveAgentSend {

            /** ResolveAgentSend token */
            token?: (string|null);

            /** ResolveAgentSend delivered */
            delivered?: (boolean|null);

            /** ResolveAgentSend dismiss */
            dismiss?: (boolean|null);
        }

        /** Represents a ResolveAgentSend. */
        class ResolveAgentSend implements IResolveAgentSend {

            /**
             * Constructs a new ResolveAgentSend.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.app.IResolveAgentSend);

            /** ResolveAgentSend token. */
            public token: string;

            /** ResolveAgentSend delivered. */
            public delivered: boolean;

            /** ResolveAgentSend dismiss. */
            public dismiss: boolean;

            /**
             * Creates a new ResolveAgentSend instance using the specified properties.
             * @param [properties] Properties to set
             * @returns ResolveAgentSend instance
             */
            public static create(properties?: muster.app.IResolveAgentSend): muster.app.ResolveAgentSend;

            /**
             * Encodes the specified ResolveAgentSend message. Does not implicitly {@link muster.app.ResolveAgentSend.verify|verify} messages.
             * @param message ResolveAgentSend message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.app.IResolveAgentSend, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a ResolveAgentSend message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns ResolveAgentSend
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.app.ResolveAgentSend;

            /**
             * Gets the default type url for ResolveAgentSend
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of an AgentSend. */
        interface IAgentSend {

            /** AgentSend text */
            text?: (string|null);

            /** AgentSend attachments */
            attachments?: (muster.v1.IAgentAttachment[]|null);

            /** AgentSend draftId */
            draftId?: (string|null);

            /** AgentSend releaseUnconfirmed */
            releaseUnconfirmed?: (string|null);
        }

        /** Represents an AgentSend. */
        class AgentSend implements IAgentSend {

            /**
             * Constructs a new AgentSend.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.app.IAgentSend);

            /** AgentSend text. */
            public text: string;

            /** AgentSend attachments. */
            public attachments: muster.v1.IAgentAttachment[];

            /** AgentSend draftId. */
            public draftId: string;

            /** AgentSend releaseUnconfirmed. */
            public releaseUnconfirmed: string;

            /**
             * Creates a new AgentSend instance using the specified properties.
             * @param [properties] Properties to set
             * @returns AgentSend instance
             */
            public static create(properties?: muster.app.IAgentSend): muster.app.AgentSend;

            /**
             * Encodes the specified AgentSend message. Does not implicitly {@link muster.app.AgentSend.verify|verify} messages.
             * @param message AgentSend message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.app.IAgentSend, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes an AgentSend message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns AgentSend
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.app.AgentSend;

            /**
             * Gets the default type url for AgentSend
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a ChatDraft. */
        interface IChatDraft {

            /** ChatDraft key */
            key?: (string|null);

            /** ChatDraft text */
            text?: (string|null);

            /** ChatDraft revision */
            revision?: (string|null);
        }

        /** Represents a ChatDraft. */
        class ChatDraft implements IChatDraft {

            /**
             * Constructs a new ChatDraft.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.app.IChatDraft);

            /** ChatDraft key. */
            public key: string;

            /** ChatDraft text. */
            public text: string;

            /** ChatDraft revision. */
            public revision: string;

            /**
             * Creates a new ChatDraft instance using the specified properties.
             * @param [properties] Properties to set
             * @returns ChatDraft instance
             */
            public static create(properties?: muster.app.IChatDraft): muster.app.ChatDraft;

            /**
             * Encodes the specified ChatDraft message. Does not implicitly {@link muster.app.ChatDraft.verify|verify} messages.
             * @param message ChatDraft message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.app.IChatDraft, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a ChatDraft message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns ChatDraft
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.app.ChatDraft;

            /**
             * Gets the default type url for ChatDraft
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a ComposerAction. */
        interface IComposerAction {

            /** ComposerAction draft */
            draft?: (muster.app.IChatDraft|null);

            /** ComposerAction action */
            action?: (muster.app.IIntent|null);
        }

        /** Represents a ComposerAction. */
        class ComposerAction implements IComposerAction {

            /**
             * Constructs a new ComposerAction.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.app.IComposerAction);

            /** ComposerAction draft. */
            public draft?: (muster.app.IChatDraft|null);

            /** ComposerAction action. */
            public action?: (muster.app.IIntent|null);

            /**
             * Creates a new ComposerAction instance using the specified properties.
             * @param [properties] Properties to set
             * @returns ComposerAction instance
             */
            public static create(properties?: muster.app.IComposerAction): muster.app.ComposerAction;

            /**
             * Encodes the specified ComposerAction message. Does not implicitly {@link muster.app.ComposerAction.verify|verify} messages.
             * @param message ComposerAction message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.app.IComposerAction, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a ComposerAction message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns ComposerAction
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.app.ComposerAction;

            /**
             * Gets the default type url for ComposerAction
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of an AgentRespond. */
        interface IAgentRespond {

            /** AgentRespond interaction */
            interaction?: (string|null);

            /** AgentRespond option */
            option?: (string|null);

            /** AgentRespond answer */
            answer?: (string|null);

            /** AgentRespond answers */
            answers?: (muster.v1.IQuestionAnswer[]|null);
        }

        /** Represents an AgentRespond. */
        class AgentRespond implements IAgentRespond {

            /**
             * Constructs a new AgentRespond.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.app.IAgentRespond);

            /** AgentRespond interaction. */
            public interaction: string;

            /** AgentRespond option. */
            public option: string;

            /** AgentRespond answer. */
            public answer: string;

            /** AgentRespond answers. */
            public answers: muster.v1.IQuestionAnswer[];

            /**
             * Creates a new AgentRespond instance using the specified properties.
             * @param [properties] Properties to set
             * @returns AgentRespond instance
             */
            public static create(properties?: muster.app.IAgentRespond): muster.app.AgentRespond;

            /**
             * Encodes the specified AgentRespond message. Does not implicitly {@link muster.app.AgentRespond.verify|verify} messages.
             * @param message AgentRespond message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.app.IAgentRespond, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes an AgentRespond message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns AgentRespond
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.app.AgentRespond;

            /**
             * Gets the default type url for AgentRespond
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of an AgentInterrupt. */
        interface IAgentInterrupt {
        }

        /** Represents an AgentInterrupt. */
        class AgentInterrupt implements IAgentInterrupt {

            /**
             * Constructs a new AgentInterrupt.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.app.IAgentInterrupt);

            /**
             * Creates a new AgentInterrupt instance using the specified properties.
             * @param [properties] Properties to set
             * @returns AgentInterrupt instance
             */
            public static create(properties?: muster.app.IAgentInterrupt): muster.app.AgentInterrupt;

            /**
             * Encodes the specified AgentInterrupt message. Does not implicitly {@link muster.app.AgentInterrupt.verify|verify} messages.
             * @param message AgentInterrupt message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.app.IAgentInterrupt, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes an AgentInterrupt message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns AgentInterrupt
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.app.AgentInterrupt;

            /**
             * Gets the default type url for AgentInterrupt
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of an AgentLoadEarlier. */
        interface IAgentLoadEarlier {

            /** AgentLoadEarlier all */
            all?: (boolean|null);
        }

        /** Represents an AgentLoadEarlier. */
        class AgentLoadEarlier implements IAgentLoadEarlier {

            /**
             * Constructs a new AgentLoadEarlier.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.app.IAgentLoadEarlier);

            /** AgentLoadEarlier all. */
            public all: boolean;

            /**
             * Creates a new AgentLoadEarlier instance using the specified properties.
             * @param [properties] Properties to set
             * @returns AgentLoadEarlier instance
             */
            public static create(properties?: muster.app.IAgentLoadEarlier): muster.app.AgentLoadEarlier;

            /**
             * Encodes the specified AgentLoadEarlier message. Does not implicitly {@link muster.app.AgentLoadEarlier.verify|verify} messages.
             * @param message AgentLoadEarlier message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.app.IAgentLoadEarlier, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes an AgentLoadEarlier message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns AgentLoadEarlier
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.app.AgentLoadEarlier;

            /**
             * Gets the default type url for AgentLoadEarlier
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of an OpenDesktopChats. */
        interface IOpenDesktopChats {

            /** OpenDesktopChats provider */
            provider?: (string|null);
        }

        /** Represents an OpenDesktopChats. */
        class OpenDesktopChats implements IOpenDesktopChats {

            /**
             * Constructs a new OpenDesktopChats.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.app.IOpenDesktopChats);

            /** OpenDesktopChats provider. */
            public provider: string;

            /**
             * Creates a new OpenDesktopChats instance using the specified properties.
             * @param [properties] Properties to set
             * @returns OpenDesktopChats instance
             */
            public static create(properties?: muster.app.IOpenDesktopChats): muster.app.OpenDesktopChats;

            /**
             * Encodes the specified OpenDesktopChats message. Does not implicitly {@link muster.app.OpenDesktopChats.verify|verify} messages.
             * @param message OpenDesktopChats message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.app.IOpenDesktopChats, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes an OpenDesktopChats message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns OpenDesktopChats
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.app.OpenDesktopChats;

            /**
             * Gets the default type url for OpenDesktopChats
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a ChatOptionsLoad. */
        interface IChatOptionsLoad {
        }

        /** Represents a ChatOptionsLoad. */
        class ChatOptionsLoad implements IChatOptionsLoad {

            /**
             * Constructs a new ChatOptionsLoad.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.app.IChatOptionsLoad);

            /**
             * Creates a new ChatOptionsLoad instance using the specified properties.
             * @param [properties] Properties to set
             * @returns ChatOptionsLoad instance
             */
            public static create(properties?: muster.app.IChatOptionsLoad): muster.app.ChatOptionsLoad;

            /**
             * Encodes the specified ChatOptionsLoad message. Does not implicitly {@link muster.app.ChatOptionsLoad.verify|verify} messages.
             * @param message ChatOptionsLoad message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.app.IChatOptionsLoad, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a ChatOptionsLoad message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns ChatOptionsLoad
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.app.ChatOptionsLoad;

            /**
             * Gets the default type url for ChatOptionsLoad
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a ComposerComplete. */
        interface IComposerComplete {
        }

        /** Represents a ComposerComplete. */
        class ComposerComplete implements IComposerComplete {

            /**
             * Constructs a new ComposerComplete.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.app.IComposerComplete);

            /**
             * Creates a new ComposerComplete instance using the specified properties.
             * @param [properties] Properties to set
             * @returns ComposerComplete instance
             */
            public static create(properties?: muster.app.IComposerComplete): muster.app.ComposerComplete;

            /**
             * Encodes the specified ComposerComplete message. Does not implicitly {@link muster.app.ComposerComplete.verify|verify} messages.
             * @param message ComposerComplete message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.app.IComposerComplete, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a ComposerComplete message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns ComposerComplete
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.app.ComposerComplete;

            /**
             * Gets the default type url for ComposerComplete
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a ChatSetOptions. */
        interface IChatSetOptions {

            /** ChatSetOptions model */
            model?: (string|null);

            /** ChatSetOptions effort */
            effort?: (string|null);
        }

        /** Represents a ChatSetOptions. */
        class ChatSetOptions implements IChatSetOptions {

            /**
             * Constructs a new ChatSetOptions.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.app.IChatSetOptions);

            /** ChatSetOptions model. */
            public model: string;

            /** ChatSetOptions effort. */
            public effort: string;

            /**
             * Creates a new ChatSetOptions instance using the specified properties.
             * @param [properties] Properties to set
             * @returns ChatSetOptions instance
             */
            public static create(properties?: muster.app.IChatSetOptions): muster.app.ChatSetOptions;

            /**
             * Encodes the specified ChatSetOptions message. Does not implicitly {@link muster.app.ChatSetOptions.verify|verify} messages.
             * @param message ChatSetOptions message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.app.IChatSetOptions, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a ChatSetOptions message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns ChatSetOptions
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.app.ChatSetOptions;

            /**
             * Gets the default type url for ChatSetOptions
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a DesktopChatOpen. */
        interface IDesktopChatOpen {

            /** DesktopChatOpen chat */
            chat?: (string|null);

            /** DesktopChatOpen desktop */
            desktop?: (boolean|null);
        }

        /** Represents a DesktopChatOpen. */
        class DesktopChatOpen implements IDesktopChatOpen {

            /**
             * Constructs a new DesktopChatOpen.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.app.IDesktopChatOpen);

            /** DesktopChatOpen chat. */
            public chat: string;

            /** DesktopChatOpen desktop. */
            public desktop: boolean;

            /**
             * Creates a new DesktopChatOpen instance using the specified properties.
             * @param [properties] Properties to set
             * @returns DesktopChatOpen instance
             */
            public static create(properties?: muster.app.IDesktopChatOpen): muster.app.DesktopChatOpen;

            /**
             * Encodes the specified DesktopChatOpen message. Does not implicitly {@link muster.app.DesktopChatOpen.verify|verify} messages.
             * @param message DesktopChatOpen message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.app.IDesktopChatOpen, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a DesktopChatOpen message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns DesktopChatOpen
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.app.DesktopChatOpen;

            /**
             * Gets the default type url for DesktopChatOpen
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of an OpenTerminals. */
        interface IOpenTerminals {

            /** OpenTerminals deviceId */
            deviceId?: (string|null);
        }

        /** Represents an OpenTerminals. */
        class OpenTerminals implements IOpenTerminals {

            /**
             * Constructs a new OpenTerminals.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.app.IOpenTerminals);

            /** OpenTerminals deviceId. */
            public deviceId: string;

            /**
             * Creates a new OpenTerminals instance using the specified properties.
             * @param [properties] Properties to set
             * @returns OpenTerminals instance
             */
            public static create(properties?: muster.app.IOpenTerminals): muster.app.OpenTerminals;

            /**
             * Encodes the specified OpenTerminals message. Does not implicitly {@link muster.app.OpenTerminals.verify|verify} messages.
             * @param message OpenTerminals message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.app.IOpenTerminals, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes an OpenTerminals message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns OpenTerminals
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.app.OpenTerminals;

            /**
             * Gets the default type url for OpenTerminals
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of an OpenTerminal. */
        interface IOpenTerminal {

            /** OpenTerminal session */
            session?: (string|null);

            /** OpenTerminal deviceId */
            deviceId?: (string|null);
        }

        /** Represents an OpenTerminal. */
        class OpenTerminal implements IOpenTerminal {

            /**
             * Constructs a new OpenTerminal.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.app.IOpenTerminal);

            /** OpenTerminal session. */
            public session: string;

            /** OpenTerminal deviceId. */
            public deviceId: string;

            /**
             * Creates a new OpenTerminal instance using the specified properties.
             * @param [properties] Properties to set
             * @returns OpenTerminal instance
             */
            public static create(properties?: muster.app.IOpenTerminal): muster.app.OpenTerminal;

            /**
             * Encodes the specified OpenTerminal message. Does not implicitly {@link muster.app.OpenTerminal.verify|verify} messages.
             * @param message OpenTerminal message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.app.IOpenTerminal, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes an OpenTerminal message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns OpenTerminal
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.app.OpenTerminal;

            /**
             * Gets the default type url for OpenTerminal
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a TerminalKey. */
        interface ITerminalKey {

            /** TerminalKey key */
            key?: (string|null);
        }

        /** Represents a TerminalKey. */
        class TerminalKey implements ITerminalKey {

            /**
             * Constructs a new TerminalKey.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.app.ITerminalKey);

            /** TerminalKey key. */
            public key: string;

            /**
             * Creates a new TerminalKey instance using the specified properties.
             * @param [properties] Properties to set
             * @returns TerminalKey instance
             */
            public static create(properties?: muster.app.ITerminalKey): muster.app.TerminalKey;

            /**
             * Encodes the specified TerminalKey message. Does not implicitly {@link muster.app.TerminalKey.verify|verify} messages.
             * @param message TerminalKey message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.app.ITerminalKey, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a TerminalKey message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns TerminalKey
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.app.TerminalKey;

            /**
             * Gets the default type url for TerminalKey
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a TerminalText. */
        interface ITerminalText {

            /** TerminalText text */
            text?: (string|null);
        }

        /** Represents a TerminalText. */
        class TerminalText implements ITerminalText {

            /**
             * Constructs a new TerminalText.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.app.ITerminalText);

            /** TerminalText text. */
            public text: string;

            /**
             * Creates a new TerminalText instance using the specified properties.
             * @param [properties] Properties to set
             * @returns TerminalText instance
             */
            public static create(properties?: muster.app.ITerminalText): muster.app.TerminalText;

            /**
             * Encodes the specified TerminalText message. Does not implicitly {@link muster.app.TerminalText.verify|verify} messages.
             * @param message TerminalText message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.app.ITerminalText, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a TerminalText message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns TerminalText
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.app.TerminalText;

            /**
             * Gets the default type url for TerminalText
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a TerminalCreate. */
        interface ITerminalCreate {

            /** TerminalCreate profile */
            profile?: (string|null);

            /** TerminalCreate source */
            source?: (string|null);

            /** TerminalCreate attachManaged */
            attachManaged?: (boolean|null);
        }

        /** Represents a TerminalCreate. */
        class TerminalCreate implements ITerminalCreate {

            /**
             * Constructs a new TerminalCreate.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.app.ITerminalCreate);

            /** TerminalCreate profile. */
            public profile: string;

            /** TerminalCreate source. */
            public source: string;

            /** TerminalCreate attachManaged. */
            public attachManaged: boolean;

            /**
             * Creates a new TerminalCreate instance using the specified properties.
             * @param [properties] Properties to set
             * @returns TerminalCreate instance
             */
            public static create(properties?: muster.app.ITerminalCreate): muster.app.TerminalCreate;

            /**
             * Encodes the specified TerminalCreate message. Does not implicitly {@link muster.app.TerminalCreate.verify|verify} messages.
             * @param message TerminalCreate message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.app.ITerminalCreate, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a TerminalCreate message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns TerminalCreate
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.app.TerminalCreate;

            /**
             * Gets the default type url for TerminalCreate
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of an UploadDiagnostics. */
        interface IUploadDiagnostics {

            /** UploadDiagnostics deviceId */
            deviceId?: (string|null);

            /** UploadDiagnostics kind */
            kind?: (string|null);

            /** UploadDiagnostics data */
            data?: (Uint8Array|null);
        }

        /** Represents an UploadDiagnostics. */
        class UploadDiagnostics implements IUploadDiagnostics {

            /**
             * Constructs a new UploadDiagnostics.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.app.IUploadDiagnostics);

            /** UploadDiagnostics deviceId. */
            public deviceId: string;

            /** UploadDiagnostics kind. */
            public kind: string;

            /** UploadDiagnostics data. */
            public data: Uint8Array;

            /**
             * Creates a new UploadDiagnostics instance using the specified properties.
             * @param [properties] Properties to set
             * @returns UploadDiagnostics instance
             */
            public static create(properties?: muster.app.IUploadDiagnostics): muster.app.UploadDiagnostics;

            /**
             * Encodes the specified UploadDiagnostics message. Does not implicitly {@link muster.app.UploadDiagnostics.verify|verify} messages.
             * @param message UploadDiagnostics message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.app.IUploadDiagnostics, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes an UploadDiagnostics message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns UploadDiagnostics
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.app.UploadDiagnostics;

            /**
             * Gets the default type url for UploadDiagnostics
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a TerminalPaste. */
        interface ITerminalPaste {

            /** TerminalPaste text */
            text?: (string|null);
        }

        /** Represents a TerminalPaste. */
        class TerminalPaste implements ITerminalPaste {

            /**
             * Constructs a new TerminalPaste.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.app.ITerminalPaste);

            /** TerminalPaste text. */
            public text: string;

            /**
             * Creates a new TerminalPaste instance using the specified properties.
             * @param [properties] Properties to set
             * @returns TerminalPaste instance
             */
            public static create(properties?: muster.app.ITerminalPaste): muster.app.TerminalPaste;

            /**
             * Encodes the specified TerminalPaste message. Does not implicitly {@link muster.app.TerminalPaste.verify|verify} messages.
             * @param message TerminalPaste message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.app.ITerminalPaste, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a TerminalPaste message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns TerminalPaste
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.app.TerminalPaste;

            /**
             * Gets the default type url for TerminalPaste
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a TerminalResize. */
        interface ITerminalResize {

            /** TerminalResize cols */
            cols?: (number|null);

            /** TerminalResize rows */
            rows?: (number|null);
        }

        /** Represents a TerminalResize. */
        class TerminalResize implements ITerminalResize {

            /**
             * Constructs a new TerminalResize.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.app.ITerminalResize);

            /** TerminalResize cols. */
            public cols: number;

            /** TerminalResize rows. */
            public rows: number;

            /**
             * Creates a new TerminalResize instance using the specified properties.
             * @param [properties] Properties to set
             * @returns TerminalResize instance
             */
            public static create(properties?: muster.app.ITerminalResize): muster.app.TerminalResize;

            /**
             * Encodes the specified TerminalResize message. Does not implicitly {@link muster.app.TerminalResize.verify|verify} messages.
             * @param message TerminalResize message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.app.ITerminalResize, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a TerminalResize message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns TerminalResize
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.app.TerminalResize;

            /**
             * Gets the default type url for TerminalResize
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of an OpenSsh. */
        interface IOpenSsh {

            /** OpenSsh port */
            port?: (number|null);

            /** OpenSsh user */
            user?: (string|null);

            /** OpenSsh cols */
            cols?: (number|null);

            /** OpenSsh rows */
            rows?: (number|null);
        }

        /** Represents an OpenSsh. */
        class OpenSsh implements IOpenSsh {

            /**
             * Constructs a new OpenSsh.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.app.IOpenSsh);

            /** OpenSsh port. */
            public port: number;

            /** OpenSsh user. */
            public user: string;

            /** OpenSsh cols. */
            public cols: number;

            /** OpenSsh rows. */
            public rows: number;

            /**
             * Creates a new OpenSsh instance using the specified properties.
             * @param [properties] Properties to set
             * @returns OpenSsh instance
             */
            public static create(properties?: muster.app.IOpenSsh): muster.app.OpenSsh;

            /**
             * Encodes the specified OpenSsh message. Does not implicitly {@link muster.app.OpenSsh.verify|verify} messages.
             * @param message OpenSsh message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.app.IOpenSsh, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes an OpenSsh message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns OpenSsh
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.app.OpenSsh;

            /**
             * Gets the default type url for OpenSsh
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a CloseTerminal. */
        interface ICloseTerminal {
        }

        /** Represents a CloseTerminal. */
        class CloseTerminal implements ICloseTerminal {

            /**
             * Constructs a new CloseTerminal.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.app.ICloseTerminal);

            /**
             * Creates a new CloseTerminal instance using the specified properties.
             * @param [properties] Properties to set
             * @returns CloseTerminal instance
             */
            public static create(properties?: muster.app.ICloseTerminal): muster.app.CloseTerminal;

            /**
             * Encodes the specified CloseTerminal message. Does not implicitly {@link muster.app.CloseTerminal.verify|verify} messages.
             * @param message CloseTerminal message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.app.ICloseTerminal, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a CloseTerminal message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns CloseTerminal
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.app.CloseTerminal;

            /**
             * Gets the default type url for CloseTerminal
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of an OpenFiles. */
        interface IOpenFiles {

            /** OpenFiles path */
            path?: (string|null);
        }

        /** Represents an OpenFiles. */
        class OpenFiles implements IOpenFiles {

            /**
             * Constructs a new OpenFiles.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.app.IOpenFiles);

            /** OpenFiles path. */
            public path: string;

            /**
             * Creates a new OpenFiles instance using the specified properties.
             * @param [properties] Properties to set
             * @returns OpenFiles instance
             */
            public static create(properties?: muster.app.IOpenFiles): muster.app.OpenFiles;

            /**
             * Encodes the specified OpenFiles message. Does not implicitly {@link muster.app.OpenFiles.verify|verify} messages.
             * @param message OpenFiles message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.app.IOpenFiles, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes an OpenFiles message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns OpenFiles
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.app.OpenFiles;

            /**
             * Gets the default type url for OpenFiles
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of an OpenFile. */
        interface IOpenFile {

            /** OpenFile path */
            path?: (string|null);
        }

        /** Represents an OpenFile. */
        class OpenFile implements IOpenFile {

            /**
             * Constructs a new OpenFile.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.app.IOpenFile);

            /** OpenFile path. */
            public path: string;

            /**
             * Creates a new OpenFile instance using the specified properties.
             * @param [properties] Properties to set
             * @returns OpenFile instance
             */
            public static create(properties?: muster.app.IOpenFile): muster.app.OpenFile;

            /**
             * Encodes the specified OpenFile message. Does not implicitly {@link muster.app.OpenFile.verify|verify} messages.
             * @param message OpenFile message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.app.IOpenFile, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes an OpenFile message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns OpenFile
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.app.OpenFile;

            /**
             * Gets the default type url for OpenFile
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of an OpenFileLink. */
        interface IOpenFileLink {

            /** OpenFileLink url */
            url?: (string|null);
        }

        /** Represents an OpenFileLink. */
        class OpenFileLink implements IOpenFileLink {

            /**
             * Constructs a new OpenFileLink.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.app.IOpenFileLink);

            /** OpenFileLink url. */
            public url: string;

            /**
             * Creates a new OpenFileLink instance using the specified properties.
             * @param [properties] Properties to set
             * @returns OpenFileLink instance
             */
            public static create(properties?: muster.app.IOpenFileLink): muster.app.OpenFileLink;

            /**
             * Encodes the specified OpenFileLink message. Does not implicitly {@link muster.app.OpenFileLink.verify|verify} messages.
             * @param message OpenFileLink message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.app.IOpenFileLink, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes an OpenFileLink message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns OpenFileLink
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.app.OpenFileLink;

            /**
             * Gets the default type url for OpenFileLink
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of an OpenChange. */
        interface IOpenChange {

            /** OpenChange summaryId */
            summaryId?: (string|null);

            /** OpenChange path */
            path?: (string|null);
        }

        /** Represents an OpenChange. */
        class OpenChange implements IOpenChange {

            /**
             * Constructs a new OpenChange.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.app.IOpenChange);

            /** OpenChange summaryId. */
            public summaryId: string;

            /** OpenChange path. */
            public path: string;

            /**
             * Creates a new OpenChange instance using the specified properties.
             * @param [properties] Properties to set
             * @returns OpenChange instance
             */
            public static create(properties?: muster.app.IOpenChange): muster.app.OpenChange;

            /**
             * Encodes the specified OpenChange message. Does not implicitly {@link muster.app.OpenChange.verify|verify} messages.
             * @param message OpenChange message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.app.IOpenChange, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes an OpenChange message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns OpenChange
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.app.OpenChange;

            /**
             * Gets the default type url for OpenChange
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of an OpenGit. */
        interface IOpenGit {

            /** OpenGit diffCommit */
            diffCommit?: (string|null);
        }

        /** Represents an OpenGit. */
        class OpenGit implements IOpenGit {

            /**
             * Constructs a new OpenGit.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.app.IOpenGit);

            /** OpenGit diffCommit. */
            public diffCommit: string;

            /**
             * Creates a new OpenGit instance using the specified properties.
             * @param [properties] Properties to set
             * @returns OpenGit instance
             */
            public static create(properties?: muster.app.IOpenGit): muster.app.OpenGit;

            /**
             * Encodes the specified OpenGit message. Does not implicitly {@link muster.app.OpenGit.verify|verify} messages.
             * @param message OpenGit message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.app.IOpenGit, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes an OpenGit message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns OpenGit
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.app.OpenGit;

            /**
             * Gets the default type url for OpenGit
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a SetPushToken. */
        interface ISetPushToken {

            /** SetPushToken kind */
            kind?: (number|null);

            /** SetPushToken token */
            token?: (Uint8Array|null);
        }

        /** Represents a SetPushToken. */
        class SetPushToken implements ISetPushToken {

            /**
             * Constructs a new SetPushToken.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.app.ISetPushToken);

            /** SetPushToken kind. */
            public kind: number;

            /** SetPushToken token. */
            public token: Uint8Array;

            /**
             * Creates a new SetPushToken instance using the specified properties.
             * @param [properties] Properties to set
             * @returns SetPushToken instance
             */
            public static create(properties?: muster.app.ISetPushToken): muster.app.SetPushToken;

            /**
             * Encodes the specified SetPushToken message. Does not implicitly {@link muster.app.SetPushToken.verify|verify} messages.
             * @param message SetPushToken message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.app.ISetPushToken, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a SetPushToken message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns SetPushToken
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.app.SetPushToken;

            /**
             * Gets the default type url for SetPushToken
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of an Effect. */
        interface IEffect {

            /** Effect scanQr */
            scanQr?: (muster.app.IScanQr|null);

            /** Effect authenticate */
            authenticate?: (muster.app.IAuthenticate|null);

            /** Effect notify */
            notify?: (muster.app.INotify|null);

            /** Effect openUrl */
            openUrl?: (muster.app.IOpenUrl|null);

            /** Effect cover */
            cover?: (muster.app.ICover|null);

            /** Effect localAuth */
            localAuth?: (muster.app.ILocalAuth|null);

            /** Effect authorizeDesktopHelper */
            authorizeDesktopHelper?: (muster.app.IAuthorizeDesktopHelper|null);

            /** Effect restartDesktop */
            restartDesktop?: (muster.app.IRestartDesktop|null);

            /** Effect prepareDesktopUpdate */
            prepareDesktopUpdate?: (muster.app.IPrepareDesktopUpdate|null);

            /** Effect finishDesktopUpdate */
            finishDesktopUpdate?: (muster.app.IFinishDesktopUpdate|null);

            /** Effect activateNetwork */
            activateNetwork?: (muster.app.IActivateNetwork|null);

            /** Effect sendDiagnostic */
            sendDiagnostic?: (muster.app.ISendDiagnostic|null);
        }

        /** Represents an Effect. */
        class Effect implements IEffect {

            /**
             * Constructs a new Effect.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.app.IEffect);

            /** Effect scanQr. */
            public scanQr?: (muster.app.IScanQr|null);

            /** Effect authenticate. */
            public authenticate?: (muster.app.IAuthenticate|null);

            /** Effect notify. */
            public notify?: (muster.app.INotify|null);

            /** Effect openUrl. */
            public openUrl?: (muster.app.IOpenUrl|null);

            /** Effect cover. */
            public cover?: (muster.app.ICover|null);

            /** Effect localAuth. */
            public localAuth?: (muster.app.ILocalAuth|null);

            /** Effect authorizeDesktopHelper. */
            public authorizeDesktopHelper?: (muster.app.IAuthorizeDesktopHelper|null);

            /** Effect restartDesktop. */
            public restartDesktop?: (muster.app.IRestartDesktop|null);

            /** Effect prepareDesktopUpdate. */
            public prepareDesktopUpdate?: (muster.app.IPrepareDesktopUpdate|null);

            /** Effect finishDesktopUpdate. */
            public finishDesktopUpdate?: (muster.app.IFinishDesktopUpdate|null);

            /** Effect activateNetwork. */
            public activateNetwork?: (muster.app.IActivateNetwork|null);

            /** Effect sendDiagnostic. */
            public sendDiagnostic?: (muster.app.ISendDiagnostic|null);

            /** Effect ef. */
            public ef?: ("scanQr"|"authenticate"|"notify"|"openUrl"|"cover"|"localAuth"|"authorizeDesktopHelper"|"restartDesktop"|"prepareDesktopUpdate"|"finishDesktopUpdate"|"activateNetwork"|"sendDiagnostic");

            /**
             * Creates a new Effect instance using the specified properties.
             * @param [properties] Properties to set
             * @returns Effect instance
             */
            public static create(properties?: muster.app.IEffect): muster.app.Effect;

            /**
             * Encodes the specified Effect message. Does not implicitly {@link muster.app.Effect.verify|verify} messages.
             * @param message Effect message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.app.IEffect, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes an Effect message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns Effect
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.app.Effect;

            /**
             * Gets the default type url for Effect
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a SendDiagnostic. */
        interface ISendDiagnostic {

            /** SendDiagnostic json */
            json?: (Uint8Array|null);
        }

        /** Represents a SendDiagnostic. */
        class SendDiagnostic implements ISendDiagnostic {

            /**
             * Constructs a new SendDiagnostic.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.app.ISendDiagnostic);

            /** SendDiagnostic json. */
            public json: Uint8Array;

            /**
             * Creates a new SendDiagnostic instance using the specified properties.
             * @param [properties] Properties to set
             * @returns SendDiagnostic instance
             */
            public static create(properties?: muster.app.ISendDiagnostic): muster.app.SendDiagnostic;

            /**
             * Encodes the specified SendDiagnostic message. Does not implicitly {@link muster.app.SendDiagnostic.verify|verify} messages.
             * @param message SendDiagnostic message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.app.ISendDiagnostic, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a SendDiagnostic message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns SendDiagnostic
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.app.SendDiagnostic;

            /**
             * Gets the default type url for SendDiagnostic
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a Cover. */
        interface ICover {

            /** Cover on */
            on?: (boolean|null);
        }

        /** Represents a Cover. */
        class Cover implements ICover {

            /**
             * Constructs a new Cover.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.app.ICover);

            /** Cover on. */
            public on: boolean;

            /**
             * Creates a new Cover instance using the specified properties.
             * @param [properties] Properties to set
             * @returns Cover instance
             */
            public static create(properties?: muster.app.ICover): muster.app.Cover;

            /**
             * Encodes the specified Cover message. Does not implicitly {@link muster.app.Cover.verify|verify} messages.
             * @param message Cover message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.app.ICover, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a Cover message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns Cover
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.app.Cover;

            /**
             * Gets the default type url for Cover
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a LocalAuth. */
        interface ILocalAuth {

            /** LocalAuth reason */
            reason?: (string|null);
        }

        /** Represents a LocalAuth. */
        class LocalAuth implements ILocalAuth {

            /**
             * Constructs a new LocalAuth.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.app.ILocalAuth);

            /** LocalAuth reason. */
            public reason: string;

            /**
             * Creates a new LocalAuth instance using the specified properties.
             * @param [properties] Properties to set
             * @returns LocalAuth instance
             */
            public static create(properties?: muster.app.ILocalAuth): muster.app.LocalAuth;

            /**
             * Encodes the specified LocalAuth message. Does not implicitly {@link muster.app.LocalAuth.verify|verify} messages.
             * @param message LocalAuth message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.app.ILocalAuth, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a LocalAuth message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns LocalAuth
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.app.LocalAuth;

            /**
             * Gets the default type url for LocalAuth
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a PrepareDesktopUpdate. */
        interface IPrepareDesktopUpdate {
        }

        /** Represents a PrepareDesktopUpdate. */
        class PrepareDesktopUpdate implements IPrepareDesktopUpdate {

            /**
             * Constructs a new PrepareDesktopUpdate.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.app.IPrepareDesktopUpdate);

            /**
             * Creates a new PrepareDesktopUpdate instance using the specified properties.
             * @param [properties] Properties to set
             * @returns PrepareDesktopUpdate instance
             */
            public static create(properties?: muster.app.IPrepareDesktopUpdate): muster.app.PrepareDesktopUpdate;

            /**
             * Encodes the specified PrepareDesktopUpdate message. Does not implicitly {@link muster.app.PrepareDesktopUpdate.verify|verify} messages.
             * @param message PrepareDesktopUpdate message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.app.IPrepareDesktopUpdate, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a PrepareDesktopUpdate message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns PrepareDesktopUpdate
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.app.PrepareDesktopUpdate;

            /**
             * Gets the default type url for PrepareDesktopUpdate
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of an ActivateNetwork. */
        interface IActivateNetwork {

            /** ActivateNetwork address */
            address?: (string|null);

            /** ActivateNetwork port */
            port?: (number|null);

            /** ActivateNetwork timeoutMs */
            timeoutMs?: (number|null);
        }

        /** Represents an ActivateNetwork. */
        class ActivateNetwork implements IActivateNetwork {

            /**
             * Constructs a new ActivateNetwork.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.app.IActivateNetwork);

            /** ActivateNetwork address. */
            public address: string;

            /** ActivateNetwork port. */
            public port: number;

            /** ActivateNetwork timeoutMs. */
            public timeoutMs: number;

            /**
             * Creates a new ActivateNetwork instance using the specified properties.
             * @param [properties] Properties to set
             * @returns ActivateNetwork instance
             */
            public static create(properties?: muster.app.IActivateNetwork): muster.app.ActivateNetwork;

            /**
             * Encodes the specified ActivateNetwork message. Does not implicitly {@link muster.app.ActivateNetwork.verify|verify} messages.
             * @param message ActivateNetwork message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.app.IActivateNetwork, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes an ActivateNetwork message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns ActivateNetwork
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.app.ActivateNetwork;

            /**
             * Gets the default type url for ActivateNetwork
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a FinishDesktopUpdate. */
        interface IFinishDesktopUpdate {

            /** FinishDesktopUpdate exit */
            exit?: (boolean|null);
        }

        /** Represents a FinishDesktopUpdate. */
        class FinishDesktopUpdate implements IFinishDesktopUpdate {

            /**
             * Constructs a new FinishDesktopUpdate.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.app.IFinishDesktopUpdate);

            /** FinishDesktopUpdate exit. */
            public exit: boolean;

            /**
             * Creates a new FinishDesktopUpdate instance using the specified properties.
             * @param [properties] Properties to set
             * @returns FinishDesktopUpdate instance
             */
            public static create(properties?: muster.app.IFinishDesktopUpdate): muster.app.FinishDesktopUpdate;

            /**
             * Encodes the specified FinishDesktopUpdate message. Does not implicitly {@link muster.app.FinishDesktopUpdate.verify|verify} messages.
             * @param message FinishDesktopUpdate message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.app.IFinishDesktopUpdate, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a FinishDesktopUpdate message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns FinishDesktopUpdate
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.app.FinishDesktopUpdate;

            /**
             * Gets the default type url for FinishDesktopUpdate
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a RestartDesktop. */
        interface IRestartDesktop {
        }

        /** Represents a RestartDesktop. */
        class RestartDesktop implements IRestartDesktop {

            /**
             * Constructs a new RestartDesktop.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.app.IRestartDesktop);

            /**
             * Creates a new RestartDesktop instance using the specified properties.
             * @param [properties] Properties to set
             * @returns RestartDesktop instance
             */
            public static create(properties?: muster.app.IRestartDesktop): muster.app.RestartDesktop;

            /**
             * Encodes the specified RestartDesktop message. Does not implicitly {@link muster.app.RestartDesktop.verify|verify} messages.
             * @param message RestartDesktop message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.app.IRestartDesktop, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a RestartDesktop message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns RestartDesktop
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.app.RestartDesktop;

            /**
             * Gets the default type url for RestartDesktop
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of an AuthorizeDesktopHelper. */
        interface IAuthorizeDesktopHelper {
        }

        /** Represents an AuthorizeDesktopHelper. */
        class AuthorizeDesktopHelper implements IAuthorizeDesktopHelper {

            /**
             * Constructs a new AuthorizeDesktopHelper.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.app.IAuthorizeDesktopHelper);

            /**
             * Creates a new AuthorizeDesktopHelper instance using the specified properties.
             * @param [properties] Properties to set
             * @returns AuthorizeDesktopHelper instance
             */
            public static create(properties?: muster.app.IAuthorizeDesktopHelper): muster.app.AuthorizeDesktopHelper;

            /**
             * Encodes the specified AuthorizeDesktopHelper message. Does not implicitly {@link muster.app.AuthorizeDesktopHelper.verify|verify} messages.
             * @param message AuthorizeDesktopHelper message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.app.IAuthorizeDesktopHelper, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes an AuthorizeDesktopHelper message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns AuthorizeDesktopHelper
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.app.AuthorizeDesktopHelper;

            /**
             * Gets the default type url for AuthorizeDesktopHelper
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a ScanQr. */
        interface IScanQr {
        }

        /** Represents a ScanQr. */
        class ScanQr implements IScanQr {

            /**
             * Constructs a new ScanQr.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.app.IScanQr);

            /**
             * Creates a new ScanQr instance using the specified properties.
             * @param [properties] Properties to set
             * @returns ScanQr instance
             */
            public static create(properties?: muster.app.IScanQr): muster.app.ScanQr;

            /**
             * Encodes the specified ScanQr message. Does not implicitly {@link muster.app.ScanQr.verify|verify} messages.
             * @param message ScanQr message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.app.IScanQr, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a ScanQr message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns ScanQr
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.app.ScanQr;

            /**
             * Gets the default type url for ScanQr
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of an Authenticate. */
        interface IAuthenticate {

            /** Authenticate reason */
            reason?: (string|null);

            /** Authenticate message */
            message?: (Uint8Array|null);
        }

        /** Represents an Authenticate. */
        class Authenticate implements IAuthenticate {

            /**
             * Constructs a new Authenticate.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.app.IAuthenticate);

            /** Authenticate reason. */
            public reason: string;

            /** Authenticate message. */
            public message: Uint8Array;

            /**
             * Creates a new Authenticate instance using the specified properties.
             * @param [properties] Properties to set
             * @returns Authenticate instance
             */
            public static create(properties?: muster.app.IAuthenticate): muster.app.Authenticate;

            /**
             * Encodes the specified Authenticate message. Does not implicitly {@link muster.app.Authenticate.verify|verify} messages.
             * @param message Authenticate message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.app.IAuthenticate, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes an Authenticate message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns Authenticate
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.app.Authenticate;

            /**
             * Gets the default type url for Authenticate
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a Notify. */
        interface INotify {

            /** Notify title */
            title?: (string|null);

            /** Notify body */
            body?: (string|null);

            /** Notify session */
            session?: (string|null);
        }

        /** Represents a Notify. */
        class Notify implements INotify {

            /**
             * Constructs a new Notify.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.app.INotify);

            /** Notify title. */
            public title: string;

            /** Notify body. */
            public body: string;

            /** Notify session. */
            public session: string;

            /**
             * Creates a new Notify instance using the specified properties.
             * @param [properties] Properties to set
             * @returns Notify instance
             */
            public static create(properties?: muster.app.INotify): muster.app.Notify;

            /**
             * Encodes the specified Notify message. Does not implicitly {@link muster.app.Notify.verify|verify} messages.
             * @param message Notify message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.app.INotify, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a Notify message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns Notify
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.app.Notify;

            /**
             * Gets the default type url for Notify
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of an OpenUrl. */
        interface IOpenUrl {

            /** OpenUrl url */
            url?: (string|null);
        }

        /** Represents an OpenUrl. */
        class OpenUrl implements IOpenUrl {

            /**
             * Constructs a new OpenUrl.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.app.IOpenUrl);

            /** OpenUrl url. */
            public url: string;

            /**
             * Creates a new OpenUrl instance using the specified properties.
             * @param [properties] Properties to set
             * @returns OpenUrl instance
             */
            public static create(properties?: muster.app.IOpenUrl): muster.app.OpenUrl;

            /**
             * Encodes the specified OpenUrl message. Does not implicitly {@link muster.app.OpenUrl.verify|verify} messages.
             * @param message OpenUrl message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.app.IOpenUrl, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes an OpenUrl message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns OpenUrl
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.app.OpenUrl;

            /**
             * Gets the default type url for OpenUrl
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of an EffectResult. */
        interface IEffectResult {

            /** EffectResult ok */
            ok?: (boolean|null);

            /** EffectResult error */
            error?: (string|null);

            /** EffectResult data */
            data?: (Uint8Array|null);

            /** EffectResult text */
            text?: (string|null);
        }

        /** Represents an EffectResult. */
        class EffectResult implements IEffectResult {

            /**
             * Constructs a new EffectResult.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.app.IEffectResult);

            /** EffectResult ok. */
            public ok: boolean;

            /** EffectResult error. */
            public error: string;

            /** EffectResult data. */
            public data: Uint8Array;

            /** EffectResult text. */
            public text: string;

            /**
             * Creates a new EffectResult instance using the specified properties.
             * @param [properties] Properties to set
             * @returns EffectResult instance
             */
            public static create(properties?: muster.app.IEffectResult): muster.app.EffectResult;

            /**
             * Encodes the specified EffectResult message. Does not implicitly {@link muster.app.EffectResult.verify|verify} messages.
             * @param message EffectResult message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.app.IEffectResult, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes an EffectResult message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns EffectResult
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.app.EffectResult;

            /**
             * Gets the default type url for EffectResult
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Lifecycle enum. */
        enum Lifecycle {
            FOREGROUND = 0,
            BACKGROUND = 1,
            NETWORK_CHANGED = 2,
            TERMINATING = 3
        }

        /** Screen enum. */
        enum Screen {
            ONBOARDING = 0,
            HOME = 1,
            INVITE = 2,
            JOIN = 3,
            DEVICE = 4,
            AGENTS = 5,
            CHAT = 6,
            TERMINALS = 7,
            TERMINAL = 8,
            SETTINGS = 9,
            FILES = 10,
            FILE = 11,
            GIT = 12,
            BROWSER = 13,
            DESKTOP_CHATS = 14,
            MULTICA = 15
        }

        /** Properties of a ViewModel. */
        interface IViewModel {

            /** ViewModel screen */
            screen?: (muster.app.Screen|null);

            /** ViewModel toasts */
            toasts?: (muster.app.IToast[]|null);

            /** ViewModel busy */
            busy?: (boolean|null);

            /** ViewModel self */
            self?: (muster.v1.IDeviceInfo|null);

            /** ViewModel network */
            network?: (muster.v1.INetworkStatus|null);

            /** ViewModel pending */
            pending?: (muster.v1.IPendingJoin[]|null);

            /** ViewModel selectedDevice */
            selectedDevice?: (string|null);

            /** ViewModel invite */
            invite?: (muster.app.IInviteView|null);

            /** ViewModel join */
            join?: (muster.app.IJoinView|null);

            /** ViewModel agents */
            agents?: (muster.app.IAgentsView|null);

            /** ViewModel chat */
            chat?: (muster.app.IChatView|null);

            /** ViewModel terminals */
            terminals?: (muster.app.ITerminalsView|null);

            /** ViewModel terminal */
            terminal?: (muster.app.ITerminalView|null);

            /** ViewModel policy */
            policy?: (muster.v1.IPolicy|null);

            /** ViewModel embedded */
            embedded?: (boolean|null);

            /** ViewModel files */
            files?: (muster.app.IFilesView|null);

            /** ViewModel file */
            file?: (muster.app.IFileView|null);

            /** ViewModel git */
            git?: (muster.app.IGitView|null);

            /** ViewModel serviceDevices */
            serviceDevices?: (muster.v1.IMemberView[]|null);

            /** ViewModel selectedDeviceHasServices */
            selectedDeviceHasServices?: (boolean|null);

            /** ViewModel localDeviceHasServices */
            localDeviceHasServices?: (boolean|null);

            /** ViewModel browser */
            browser?: (muster.app.IBrowserView|null);

            /** ViewModel canGoBack */
            canGoBack?: (boolean|null);

            /** ViewModel desktopSetup */
            desktopSetup?: (muster.app.IDesktopSetupView|null);

            /** ViewModel desktopUpdate */
            desktopUpdate?: (muster.app.IDesktopUpdateView|null);

            /** ViewModel desktopChats */
            desktopChats?: (muster.app.IDesktopChatsView|null);

            /** ViewModel workspace */
            workspace?: (muster.app.Screen|null);

            /** ViewModel canInvite */
            canInvite?: (boolean|null);

            /** ViewModel deviceAccess */
            deviceAccess?: (muster.app.IAccessView|null);

            /** ViewModel joinAccess */
            joinAccess?: (muster.app.IAccessView|null);

            /** ViewModel deviceLockedUse */
            deviceLockedUse?: (muster.app.IDeviceLockedUseView[]|null);

            /** ViewModel deviceActions */
            deviceActions?: (muster.app.IDeviceActions|null);

            /** ViewModel multica */
            multica?: (muster.v1.IMulticaStatus|null);

            /** ViewModel multicaPage */
            multicaPage?: (muster.v1.IMulticaPage|null);

            /** ViewModel navigationPath */
            navigationPath?: (muster.app.Screen[]|null);
        }

        /** Represents a ViewModel. */
        class ViewModel implements IViewModel {

            /**
             * Constructs a new ViewModel.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.app.IViewModel);

            /** ViewModel screen. */
            public screen: muster.app.Screen;

            /** ViewModel toasts. */
            public toasts: muster.app.IToast[];

            /** ViewModel busy. */
            public busy: boolean;

            /** ViewModel self. */
            public self?: (muster.v1.IDeviceInfo|null);

            /** ViewModel network. */
            public network?: (muster.v1.INetworkStatus|null);

            /** ViewModel pending. */
            public pending: muster.v1.IPendingJoin[];

            /** ViewModel selectedDevice. */
            public selectedDevice: string;

            /** ViewModel invite. */
            public invite?: (muster.app.IInviteView|null);

            /** ViewModel join. */
            public join?: (muster.app.IJoinView|null);

            /** ViewModel agents. */
            public agents?: (muster.app.IAgentsView|null);

            /** ViewModel chat. */
            public chat?: (muster.app.IChatView|null);

            /** ViewModel terminals. */
            public terminals?: (muster.app.ITerminalsView|null);

            /** ViewModel terminal. */
            public terminal?: (muster.app.ITerminalView|null);

            /** ViewModel policy. */
            public policy?: (muster.v1.IPolicy|null);

            /** ViewModel embedded. */
            public embedded: boolean;

            /** ViewModel files. */
            public files?: (muster.app.IFilesView|null);

            /** ViewModel file. */
            public file?: (muster.app.IFileView|null);

            /** ViewModel git. */
            public git?: (muster.app.IGitView|null);

            /** ViewModel serviceDevices. */
            public serviceDevices: muster.v1.IMemberView[];

            /** ViewModel selectedDeviceHasServices. */
            public selectedDeviceHasServices: boolean;

            /** ViewModel localDeviceHasServices. */
            public localDeviceHasServices: boolean;

            /** ViewModel browser. */
            public browser?: (muster.app.IBrowserView|null);

            /** ViewModel canGoBack. */
            public canGoBack: boolean;

            /** ViewModel desktopSetup. */
            public desktopSetup?: (muster.app.IDesktopSetupView|null);

            /** ViewModel desktopUpdate. */
            public desktopUpdate?: (muster.app.IDesktopUpdateView|null);

            /** ViewModel desktopChats. */
            public desktopChats?: (muster.app.IDesktopChatsView|null);

            /** ViewModel workspace. */
            public workspace: muster.app.Screen;

            /** ViewModel canInvite. */
            public canInvite: boolean;

            /** ViewModel deviceAccess. */
            public deviceAccess?: (muster.app.IAccessView|null);

            /** ViewModel joinAccess. */
            public joinAccess?: (muster.app.IAccessView|null);

            /** ViewModel deviceLockedUse. */
            public deviceLockedUse: muster.app.IDeviceLockedUseView[];

            /** ViewModel deviceActions. */
            public deviceActions?: (muster.app.IDeviceActions|null);

            /** ViewModel multica. */
            public multica?: (muster.v1.IMulticaStatus|null);

            /** ViewModel multicaPage. */
            public multicaPage?: (muster.v1.IMulticaPage|null);

            /** ViewModel navigationPath. */
            public navigationPath: muster.app.Screen[];

            /**
             * Creates a new ViewModel instance using the specified properties.
             * @param [properties] Properties to set
             * @returns ViewModel instance
             */
            public static create(properties?: muster.app.IViewModel): muster.app.ViewModel;

            /**
             * Encodes the specified ViewModel message. Does not implicitly {@link muster.app.ViewModel.verify|verify} messages.
             * @param message ViewModel message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.app.IViewModel, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a ViewModel message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns ViewModel
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.app.ViewModel;

            /**
             * Gets the default type url for ViewModel
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a DeviceActions. */
        interface IDeviceActions {

            /** DeviceActions canCover */
            canCover?: (boolean|null);

            /** DeviceActions canSsh */
            canSsh?: (boolean|null);

            /** DeviceActions showAccess */
            showAccess?: (boolean|null);

            /** DeviceActions canEditAccess */
            canEditAccess?: (boolean|null);

            /** DeviceActions canRemove */
            canRemove?: (boolean|null);
        }

        /** Represents a DeviceActions. */
        class DeviceActions implements IDeviceActions {

            /**
             * Constructs a new DeviceActions.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.app.IDeviceActions);

            /** DeviceActions canCover. */
            public canCover: boolean;

            /** DeviceActions canSsh. */
            public canSsh: boolean;

            /** DeviceActions showAccess. */
            public showAccess: boolean;

            /** DeviceActions canEditAccess. */
            public canEditAccess: boolean;

            /** DeviceActions canRemove. */
            public canRemove: boolean;

            /**
             * Creates a new DeviceActions instance using the specified properties.
             * @param [properties] Properties to set
             * @returns DeviceActions instance
             */
            public static create(properties?: muster.app.IDeviceActions): muster.app.DeviceActions;

            /**
             * Encodes the specified DeviceActions message. Does not implicitly {@link muster.app.DeviceActions.verify|verify} messages.
             * @param message DeviceActions message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.app.IDeviceActions, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a DeviceActions message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns DeviceActions
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.app.DeviceActions;

            /**
             * Gets the default type url for DeviceActions
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a DesktopChatsView. */
        interface IDesktopChatsView {

            /** DesktopChatsView deviceId */
            deviceId?: (string|null);

            /** DesktopChatsView deviceName */
            deviceName?: (string|null);

            /** DesktopChatsView provider */
            provider?: (string|null);

            /** DesktopChatsView catalog */
            catalog?: (muster.v1.IAgentChatCatalog|null);

            /** DesktopChatsView loading */
            loading?: (boolean|null);

            /** DesktopChatsView error */
            error?: (string|null);

            /** DesktopChatsView opening */
            opening?: (string|null);

            /** DesktopChatsView choices */
            choices?: (muster.app.IChatProvider[]|null);
        }

        /** Represents a DesktopChatsView. */
        class DesktopChatsView implements IDesktopChatsView {

            /**
             * Constructs a new DesktopChatsView.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.app.IDesktopChatsView);

            /** DesktopChatsView deviceId. */
            public deviceId: string;

            /** DesktopChatsView deviceName. */
            public deviceName: string;

            /** DesktopChatsView provider. */
            public provider: string;

            /** DesktopChatsView catalog. */
            public catalog?: (muster.v1.IAgentChatCatalog|null);

            /** DesktopChatsView loading. */
            public loading: boolean;

            /** DesktopChatsView error. */
            public error: string;

            /** DesktopChatsView opening. */
            public opening: string;

            /** DesktopChatsView choices. */
            public choices: muster.app.IChatProvider[];

            /**
             * Creates a new DesktopChatsView instance using the specified properties.
             * @param [properties] Properties to set
             * @returns DesktopChatsView instance
             */
            public static create(properties?: muster.app.IDesktopChatsView): muster.app.DesktopChatsView;

            /**
             * Encodes the specified DesktopChatsView message. Does not implicitly {@link muster.app.DesktopChatsView.verify|verify} messages.
             * @param message DesktopChatsView message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.app.IDesktopChatsView, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a DesktopChatsView message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns DesktopChatsView
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.app.DesktopChatsView;

            /**
             * Gets the default type url for DesktopChatsView
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a ChatProvider. */
        interface IChatProvider {

            /** ChatProvider id */
            id?: (string|null);

            /** ChatProvider label */
            label?: (string|null);
        }

        /** Represents a ChatProvider. */
        class ChatProvider implements IChatProvider {

            /**
             * Constructs a new ChatProvider.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.app.IChatProvider);

            /** ChatProvider id. */
            public id: string;

            /** ChatProvider label. */
            public label: string;

            /**
             * Creates a new ChatProvider instance using the specified properties.
             * @param [properties] Properties to set
             * @returns ChatProvider instance
             */
            public static create(properties?: muster.app.IChatProvider): muster.app.ChatProvider;

            /**
             * Encodes the specified ChatProvider message. Does not implicitly {@link muster.app.ChatProvider.verify|verify} messages.
             * @param message ChatProvider message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.app.IChatProvider, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a ChatProvider message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns ChatProvider
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.app.ChatProvider;

            /**
             * Gets the default type url for ChatProvider
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of an AccessView. */
        interface IAccessView {

            /** AccessView sections */
            sections?: (muster.app.IAccessSection[]|null);
        }

        /** Represents an AccessView. */
        class AccessView implements IAccessView {

            /**
             * Constructs a new AccessView.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.app.IAccessView);

            /** AccessView sections. */
            public sections: muster.app.IAccessSection[];

            /**
             * Creates a new AccessView instance using the specified properties.
             * @param [properties] Properties to set
             * @returns AccessView instance
             */
            public static create(properties?: muster.app.IAccessView): muster.app.AccessView;

            /**
             * Encodes the specified AccessView message. Does not implicitly {@link muster.app.AccessView.verify|verify} messages.
             * @param message AccessView message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.app.IAccessView, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes an AccessView message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns AccessView
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.app.AccessView;

            /**
             * Gets the default type url for AccessView
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of an AccessSection. */
        interface IAccessSection {

            /** AccessSection id */
            id?: (string|null);

            /** AccessSection title */
            title?: (string|null);

            /** AccessSection footer */
            footer?: (string|null);

            /** AccessSection summary */
            summary?: (string|null);

            /** AccessSection toggle */
            toggle?: (muster.app.IAccessItem|null);

            /** AccessSection items */
            items?: (muster.app.IAccessItem[]|null);
        }

        /** Represents an AccessSection. */
        class AccessSection implements IAccessSection {

            /**
             * Constructs a new AccessSection.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.app.IAccessSection);

            /** AccessSection id. */
            public id: string;

            /** AccessSection title. */
            public title: string;

            /** AccessSection footer. */
            public footer: string;

            /** AccessSection summary. */
            public summary: string;

            /** AccessSection toggle. */
            public toggle?: (muster.app.IAccessItem|null);

            /** AccessSection items. */
            public items: muster.app.IAccessItem[];

            /**
             * Creates a new AccessSection instance using the specified properties.
             * @param [properties] Properties to set
             * @returns AccessSection instance
             */
            public static create(properties?: muster.app.IAccessSection): muster.app.AccessSection;

            /**
             * Encodes the specified AccessSection message. Does not implicitly {@link muster.app.AccessSection.verify|verify} messages.
             * @param message AccessSection message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.app.IAccessSection, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes an AccessSection message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns AccessSection
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.app.AccessSection;

            /**
             * Gets the default type url for AccessSection
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of an AccessItem. */
        interface IAccessItem {

            /** AccessItem scope */
            scope?: (string|null);

            /** AccessItem label */
            label?: (string|null);

            /** AccessItem granted */
            granted?: (boolean|null);

            /** AccessItem requiresToggle */
            requiresToggle?: (boolean|null);
        }

        /** Represents an AccessItem. */
        class AccessItem implements IAccessItem {

            /**
             * Constructs a new AccessItem.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.app.IAccessItem);

            /** AccessItem scope. */
            public scope: string;

            /** AccessItem label. */
            public label: string;

            /** AccessItem granted. */
            public granted: boolean;

            /** AccessItem requiresToggle. */
            public requiresToggle: boolean;

            /**
             * Creates a new AccessItem instance using the specified properties.
             * @param [properties] Properties to set
             * @returns AccessItem instance
             */
            public static create(properties?: muster.app.IAccessItem): muster.app.AccessItem;

            /**
             * Encodes the specified AccessItem message. Does not implicitly {@link muster.app.AccessItem.verify|verify} messages.
             * @param message AccessItem message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.app.IAccessItem, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes an AccessItem message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns AccessItem
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.app.AccessItem;

            /**
             * Gets the default type url for AccessItem
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a DesktopSetupView. */
        interface IDesktopSetupView {

            /** DesktopSetupView visible */
            visible?: (boolean|null);

            /** DesktopSetupView accessibilityReady */
            accessibilityReady?: (boolean|null);

            /** DesktopSetupView pluginReady */
            pluginReady?: (boolean|null);

            /** DesktopSetupView checking */
            checking?: (boolean|null);

            /** DesktopSetupView canInstall */
            canInstall?: (boolean|null);

            /** DesktopSetupView pluginMessage */
            pluginMessage?: (string|null);

            /** DesktopSetupView itermReady */
            itermReady?: (boolean|null);

            /** DesktopSetupView itermMessage */
            itermMessage?: (string|null);

            /** DesktopSetupView error */
            error?: (string|null);

            /** DesktopSetupView restartRequired */
            restartRequired?: (boolean|null);

            /** DesktopSetupView canRestart */
            canRestart?: (boolean|null);

            /** DesktopSetupView showIterm */
            showIterm?: (boolean|null);

            /** DesktopSetupView ready */
            ready?: (boolean|null);

            /** DesktopSetupView terminalApp */
            terminalApp?: (string|null);
        }

        /** Represents a DesktopSetupView. */
        class DesktopSetupView implements IDesktopSetupView {

            /**
             * Constructs a new DesktopSetupView.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.app.IDesktopSetupView);

            /** DesktopSetupView visible. */
            public visible: boolean;

            /** DesktopSetupView accessibilityReady. */
            public accessibilityReady: boolean;

            /** DesktopSetupView pluginReady. */
            public pluginReady: boolean;

            /** DesktopSetupView checking. */
            public checking: boolean;

            /** DesktopSetupView canInstall. */
            public canInstall: boolean;

            /** DesktopSetupView pluginMessage. */
            public pluginMessage: string;

            /** DesktopSetupView itermReady. */
            public itermReady: boolean;

            /** DesktopSetupView itermMessage. */
            public itermMessage: string;

            /** DesktopSetupView error. */
            public error: string;

            /** DesktopSetupView restartRequired. */
            public restartRequired: boolean;

            /** DesktopSetupView canRestart. */
            public canRestart: boolean;

            /** DesktopSetupView showIterm. */
            public showIterm: boolean;

            /** DesktopSetupView ready. */
            public ready: boolean;

            /** DesktopSetupView terminalApp. */
            public terminalApp: string;

            /**
             * Creates a new DesktopSetupView instance using the specified properties.
             * @param [properties] Properties to set
             * @returns DesktopSetupView instance
             */
            public static create(properties?: muster.app.IDesktopSetupView): muster.app.DesktopSetupView;

            /**
             * Encodes the specified DesktopSetupView message. Does not implicitly {@link muster.app.DesktopSetupView.verify|verify} messages.
             * @param message DesktopSetupView message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.app.IDesktopSetupView, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a DesktopSetupView message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns DesktopSetupView
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.app.DesktopSetupView;

            /**
             * Gets the default type url for DesktopSetupView
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a Toast. */
        interface IToast {

            /** Toast id */
            id?: (string|null);

            /** Toast text */
            text?: (string|null);

            /** Toast error */
            error?: (boolean|null);
        }

        /** Represents a Toast. */
        class Toast implements IToast {

            /**
             * Constructs a new Toast.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.app.IToast);

            /** Toast id. */
            public id: string;

            /** Toast text. */
            public text: string;

            /** Toast error. */
            public error: boolean;

            /**
             * Creates a new Toast instance using the specified properties.
             * @param [properties] Properties to set
             * @returns Toast instance
             */
            public static create(properties?: muster.app.IToast): muster.app.Toast;

            /**
             * Encodes the specified Toast message. Does not implicitly {@link muster.app.Toast.verify|verify} messages.
             * @param message Toast message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.app.IToast, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a Toast message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns Toast
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.app.Toast;

            /**
             * Gets the default type url for Toast
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of an InviteView. */
        interface IInviteView {

            /** InviteView text */
            text?: (string|null);

            /** InviteView expiresUnix */
            expiresUnix?: (number|Long|null);
        }

        /** Represents an InviteView. */
        class InviteView implements IInviteView {

            /**
             * Constructs a new InviteView.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.app.IInviteView);

            /** InviteView text. */
            public text: string;

            /** InviteView expiresUnix. */
            public expiresUnix: (number|Long);

            /**
             * Creates a new InviteView instance using the specified properties.
             * @param [properties] Properties to set
             * @returns InviteView instance
             */
            public static create(properties?: muster.app.IInviteView): muster.app.InviteView;

            /**
             * Encodes the specified InviteView message. Does not implicitly {@link muster.app.InviteView.verify|verify} messages.
             * @param message InviteView message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.app.IInviteView, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes an InviteView message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns InviteView
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.app.InviteView;

            /**
             * Gets the default type url for InviteView
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a JoinView. */
        interface IJoinView {

            /** JoinView phase */
            phase?: (string|null);

            /** JoinView message */
            message?: (string|null);
        }

        /** Represents a JoinView. */
        class JoinView implements IJoinView {

            /**
             * Constructs a new JoinView.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.app.IJoinView);

            /** JoinView phase. */
            public phase: string;

            /** JoinView message. */
            public message: string;

            /**
             * Creates a new JoinView instance using the specified properties.
             * @param [properties] Properties to set
             * @returns JoinView instance
             */
            public static create(properties?: muster.app.IJoinView): muster.app.JoinView;

            /**
             * Encodes the specified JoinView message. Does not implicitly {@link muster.app.JoinView.verify|verify} messages.
             * @param message JoinView message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.app.IJoinView, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a JoinView message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns JoinView
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.app.JoinView;

            /**
             * Gets the default type url for JoinView
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of an AgentsView. */
        interface IAgentsView {

            /** AgentsView createHosts */
            createHosts?: (muster.v1.IMemberView[]|null);

            /** AgentsView createAgents */
            createAgents?: (string[]|null);

            /** AgentsView creating */
            creating?: (boolean|null);

            /** AgentsView errors */
            errors?: (string[]|null);

            /** AgentsView deviceId */
            deviceId?: (string|null);

            /** AgentsView sessions */
            sessions?: (muster.v1.IAgentSession[]|null);

            /** AgentsView loading */
            loading?: (boolean|null);
        }

        /** Represents an AgentsView. */
        class AgentsView implements IAgentsView {

            /**
             * Constructs a new AgentsView.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.app.IAgentsView);

            /** AgentsView createHosts. */
            public createHosts: muster.v1.IMemberView[];

            /** AgentsView createAgents. */
            public createAgents: string[];

            /** AgentsView creating. */
            public creating: boolean;

            /** AgentsView errors. */
            public errors: string[];

            /** AgentsView deviceId. */
            public deviceId: string;

            /** AgentsView sessions. */
            public sessions: muster.v1.IAgentSession[];

            /** AgentsView loading. */
            public loading: boolean;

            /**
             * Creates a new AgentsView instance using the specified properties.
             * @param [properties] Properties to set
             * @returns AgentsView instance
             */
            public static create(properties?: muster.app.IAgentsView): muster.app.AgentsView;

            /**
             * Encodes the specified AgentsView message. Does not implicitly {@link muster.app.AgentsView.verify|verify} messages.
             * @param message AgentsView message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.app.IAgentsView, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes an AgentsView message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns AgentsView
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.app.AgentsView;

            /**
             * Gets the default type url for AgentsView
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a ChatView. */
        interface IChatView {

            /** ChatView deviceId */
            deviceId?: (string|null);

            /** ChatView session */
            session?: (muster.v1.IAgentSession|null);

            /** ChatView interactions */
            interactions?: (muster.v1.IPendingInteraction[]|null);

            /** ChatView hasEarlier */
            hasEarlier?: (boolean|null);

            /** ChatView sending */
            sending?: (boolean|null);

            /** ChatView lockedUseStatus */
            lockedUseStatus?: (string|null);

            /** ChatView canCancelLockedUse */
            canCancelLockedUse?: (boolean|null);

            /** ChatView timeline */
            timeline?: (muster.app.IChatBlock[]|null);

            /** ChatView sentDraftId */
            sentDraftId?: (string|null);

            /** ChatView attachmentBytesLimit */
            attachmentBytesLimit?: (number|Long|null);

            /** ChatView attachmentCountLimit */
            attachmentCountLimit?: (number|null);

            /** ChatView loading */
            loading?: (boolean|null);

            /** ChatView loadError */
            loadError?: (string|null);

            /** ChatView respondingInteraction */
            respondingInteraction?: (string|null);

            /** ChatView responseError */
            responseError?: (string|null);

            /** ChatView responseUnknown */
            responseUnknown?: (boolean|null);

            /** ChatView chatOptions */
            chatOptions?: (muster.v1.IAgentChatOptions|null);

            /** ChatView chatOptionsBusy */
            chatOptionsBusy?: (boolean|null);

            /** ChatView chatOptionsError */
            chatOptionsError?: (string|null);

            /** ChatView draft */
            draft?: (muster.app.IChatDraft|null);

            /** ChatView queue */
            queue?: (muster.app.IQueuedMessagesView|null);

            /** ChatView canInterrupt */
            canInterrupt?: (boolean|null);

            /** ChatView canChooseModel */
            canChooseModel?: (boolean|null);

            /** ChatView canBrowseFiles */
            canBrowseFiles?: (boolean|null);

            /** ChatView providerLabel */
            providerLabel?: (string|null);

            /** ChatView messageCount */
            messageCount?: (number|null);

            /** ChatView unconfirmed */
            unconfirmed?: (muster.app.IUnconfirmedSend|null);

            /** ChatView contentRevision */
            contentRevision?: (number|Long|null);

            /** ChatView composerPlaceholder */
            composerPlaceholder?: (string|null);

            /** ChatView composerSuggestion */
            composerSuggestion?: (string|null);

            /** ChatView canComplete */
            canComplete?: (boolean|null);

            /** ChatView completing */
            completing?: (boolean|null);

            /** ChatView completionError */
            completionError?: (string|null);

            /** ChatView trace */
            trace?: (muster.app.ITraceSpan[]|null);

            /** ChatView terminal */
            terminal?: (muster.app.ISessionTerminalView|null);

            /** ChatView codePreview */
            codePreview?: (muster.app.ICodePreview|null);

            /** ChatView codeRuns */
            codeRuns?: (muster.app.ICodeRunView[]|null);

            /** ChatView codeError */
            codeError?: (string|null);
        }

        /** Represents a ChatView. */
        class ChatView implements IChatView {

            /**
             * Constructs a new ChatView.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.app.IChatView);

            /** ChatView deviceId. */
            public deviceId: string;

            /** ChatView session. */
            public session?: (muster.v1.IAgentSession|null);

            /** ChatView interactions. */
            public interactions: muster.v1.IPendingInteraction[];

            /** ChatView hasEarlier. */
            public hasEarlier: boolean;

            /** ChatView sending. */
            public sending: boolean;

            /** ChatView lockedUseStatus. */
            public lockedUseStatus: string;

            /** ChatView canCancelLockedUse. */
            public canCancelLockedUse: boolean;

            /** ChatView timeline. */
            public timeline: muster.app.IChatBlock[];

            /** ChatView sentDraftId. */
            public sentDraftId: string;

            /** ChatView attachmentBytesLimit. */
            public attachmentBytesLimit: (number|Long);

            /** ChatView attachmentCountLimit. */
            public attachmentCountLimit: number;

            /** ChatView loading. */
            public loading: boolean;

            /** ChatView loadError. */
            public loadError: string;

            /** ChatView respondingInteraction. */
            public respondingInteraction: string;

            /** ChatView responseError. */
            public responseError: string;

            /** ChatView responseUnknown. */
            public responseUnknown: boolean;

            /** ChatView chatOptions. */
            public chatOptions?: (muster.v1.IAgentChatOptions|null);

            /** ChatView chatOptionsBusy. */
            public chatOptionsBusy: boolean;

            /** ChatView chatOptionsError. */
            public chatOptionsError: string;

            /** ChatView draft. */
            public draft?: (muster.app.IChatDraft|null);

            /** ChatView queue. */
            public queue?: (muster.app.IQueuedMessagesView|null);

            /** ChatView canInterrupt. */
            public canInterrupt: boolean;

            /** ChatView canChooseModel. */
            public canChooseModel: boolean;

            /** ChatView canBrowseFiles. */
            public canBrowseFiles: boolean;

            /** ChatView providerLabel. */
            public providerLabel: string;

            /** ChatView messageCount. */
            public messageCount: number;

            /** ChatView unconfirmed. */
            public unconfirmed?: (muster.app.IUnconfirmedSend|null);

            /** ChatView contentRevision. */
            public contentRevision: (number|Long);

            /** ChatView composerPlaceholder. */
            public composerPlaceholder: string;

            /** ChatView composerSuggestion. */
            public composerSuggestion: string;

            /** ChatView canComplete. */
            public canComplete: boolean;

            /** ChatView completing. */
            public completing: boolean;

            /** ChatView completionError. */
            public completionError: string;

            /** ChatView trace. */
            public trace: muster.app.ITraceSpan[];

            /** ChatView terminal. */
            public terminal?: (muster.app.ISessionTerminalView|null);

            /** ChatView codePreview. */
            public codePreview?: (muster.app.ICodePreview|null);

            /** ChatView codeRuns. */
            public codeRuns: muster.app.ICodeRunView[];

            /** ChatView codeError. */
            public codeError: string;

            /**
             * Creates a new ChatView instance using the specified properties.
             * @param [properties] Properties to set
             * @returns ChatView instance
             */
            public static create(properties?: muster.app.IChatView): muster.app.ChatView;

            /**
             * Encodes the specified ChatView message. Does not implicitly {@link muster.app.ChatView.verify|verify} messages.
             * @param message ChatView message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.app.IChatView, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a ChatView message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns ChatView
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.app.ChatView;

            /**
             * Gets the default type url for ChatView
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of an UnconfirmedSend. */
        interface IUnconfirmedSend {

            /** UnconfirmedSend token */
            token?: (string|null);

            /** UnconfirmedSend message */
            message?: (string|null);
        }

        /** Represents an UnconfirmedSend. */
        class UnconfirmedSend implements IUnconfirmedSend {

            /**
             * Constructs a new UnconfirmedSend.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.app.IUnconfirmedSend);

            /** UnconfirmedSend token. */
            public token: string;

            /** UnconfirmedSend message. */
            public message: string;

            /**
             * Creates a new UnconfirmedSend instance using the specified properties.
             * @param [properties] Properties to set
             * @returns UnconfirmedSend instance
             */
            public static create(properties?: muster.app.IUnconfirmedSend): muster.app.UnconfirmedSend;

            /**
             * Encodes the specified UnconfirmedSend message. Does not implicitly {@link muster.app.UnconfirmedSend.verify|verify} messages.
             * @param message UnconfirmedSend message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.app.IUnconfirmedSend, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes an UnconfirmedSend message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns UnconfirmedSend
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.app.UnconfirmedSend;

            /**
             * Gets the default type url for UnconfirmedSend
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a QueuedMessagesView. */
        interface IQueuedMessagesView {

            /** QueuedMessagesView messages */
            messages?: (muster.v1.IPendingInteraction[]|null);

            /** QueuedMessagesView submitting */
            submitting?: (boolean|null);

            /** QueuedMessagesView error */
            error?: (string|null);

            /** QueuedMessagesView outcomeUnknown */
            outcomeUnknown?: (boolean|null);
        }

        /** Represents a QueuedMessagesView. */
        class QueuedMessagesView implements IQueuedMessagesView {

            /**
             * Constructs a new QueuedMessagesView.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.app.IQueuedMessagesView);

            /** QueuedMessagesView messages. */
            public messages: muster.v1.IPendingInteraction[];

            /** QueuedMessagesView submitting. */
            public submitting: boolean;

            /** QueuedMessagesView error. */
            public error: string;

            /** QueuedMessagesView outcomeUnknown. */
            public outcomeUnknown: boolean;

            /**
             * Creates a new QueuedMessagesView instance using the specified properties.
             * @param [properties] Properties to set
             * @returns QueuedMessagesView instance
             */
            public static create(properties?: muster.app.IQueuedMessagesView): muster.app.QueuedMessagesView;

            /**
             * Encodes the specified QueuedMessagesView message. Does not implicitly {@link muster.app.QueuedMessagesView.verify|verify} messages.
             * @param message QueuedMessagesView message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.app.IQueuedMessagesView, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a QueuedMessagesView message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns QueuedMessagesView
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.app.QueuedMessagesView;

            /**
             * Gets the default type url for QueuedMessagesView
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a ChatBlock. */
        interface IChatBlock {

            /** ChatBlock id */
            id?: (string|null);

            /** ChatBlock kind */
            kind?: (string|null);

            /** ChatBlock message */
            message?: (muster.v1.IAgentMessage|null);

            /** ChatBlock activity */
            activity?: (muster.v1.IAgentMessage[]|null);

            /** ChatBlock completed */
            completed?: (boolean|null);

            /** ChatBlock activityRows */
            activityRows?: (muster.app.IChatActivityRow[]|null);

            /** ChatBlock children */
            children?: (muster.app.IChatBlock[]|null);

            /** ChatBlock title */
            title?: (string|null);

            /** ChatBlock defaultExpanded */
            defaultExpanded?: (boolean|null);

            /** ChatBlock diffs */
            diffs?: (muster.app.IChatDiffFile[]|null);

            /** ChatBlock parts */
            parts?: (muster.app.IMessagePart[]|null);
        }

        /** Represents a ChatBlock. */
        class ChatBlock implements IChatBlock {

            /**
             * Constructs a new ChatBlock.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.app.IChatBlock);

            /** ChatBlock id. */
            public id: string;

            /** ChatBlock kind. */
            public kind: string;

            /** ChatBlock message. */
            public message?: (muster.v1.IAgentMessage|null);

            /** ChatBlock activity. */
            public activity: muster.v1.IAgentMessage[];

            /** ChatBlock completed. */
            public completed: boolean;

            /** ChatBlock activityRows. */
            public activityRows: muster.app.IChatActivityRow[];

            /** ChatBlock children. */
            public children: muster.app.IChatBlock[];

            /** ChatBlock title. */
            public title: string;

            /** ChatBlock defaultExpanded. */
            public defaultExpanded: boolean;

            /** ChatBlock diffs. */
            public diffs: muster.app.IChatDiffFile[];

            /** ChatBlock parts. */
            public parts: muster.app.IMessagePart[];

            /**
             * Creates a new ChatBlock instance using the specified properties.
             * @param [properties] Properties to set
             * @returns ChatBlock instance
             */
            public static create(properties?: muster.app.IChatBlock): muster.app.ChatBlock;

            /**
             * Encodes the specified ChatBlock message. Does not implicitly {@link muster.app.ChatBlock.verify|verify} messages.
             * @param message ChatBlock message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.app.IChatBlock, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a ChatBlock message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns ChatBlock
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.app.ChatBlock;

            /**
             * Gets the default type url for ChatBlock
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a ChatActivityRow. */
        interface IChatActivityRow {

            /** ChatActivityRow id */
            id?: (string|null);

            /** ChatActivityRow title */
            title?: (string|null);

            /** ChatActivityRow detail */
            detail?: (string|null);

            /** ChatActivityRow category */
            category?: (string|null);

            /** ChatActivityRow toolName */
            toolName?: (string|null);

            /** ChatActivityRow isResult */
            isResult?: (boolean|null);

            /** ChatActivityRow output */
            output?: (string|null);

            /** ChatActivityRow status */
            status?: (string|null);
        }

        /** Represents a ChatActivityRow. */
        class ChatActivityRow implements IChatActivityRow {

            /**
             * Constructs a new ChatActivityRow.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.app.IChatActivityRow);

            /** ChatActivityRow id. */
            public id: string;

            /** ChatActivityRow title. */
            public title: string;

            /** ChatActivityRow detail. */
            public detail: string;

            /** ChatActivityRow category. */
            public category: string;

            /** ChatActivityRow toolName. */
            public toolName: string;

            /** ChatActivityRow isResult. */
            public isResult: boolean;

            /** ChatActivityRow output. */
            public output: string;

            /** ChatActivityRow status. */
            public status: string;

            /**
             * Creates a new ChatActivityRow instance using the specified properties.
             * @param [properties] Properties to set
             * @returns ChatActivityRow instance
             */
            public static create(properties?: muster.app.IChatActivityRow): muster.app.ChatActivityRow;

            /**
             * Encodes the specified ChatActivityRow message. Does not implicitly {@link muster.app.ChatActivityRow.verify|verify} messages.
             * @param message ChatActivityRow message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.app.IChatActivityRow, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a ChatActivityRow message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns ChatActivityRow
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.app.ChatActivityRow;

            /**
             * Gets the default type url for ChatActivityRow
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a ChatDiffFile. */
        interface IChatDiffFile {

            /** ChatDiffFile id */
            id?: (string|null);

            /** ChatDiffFile path */
            path?: (string|null);

            /** ChatDiffFile added */
            added?: (number|null);

            /** ChatDiffFile removed */
            removed?: (number|null);

            /** ChatDiffFile lines */
            lines?: (muster.app.IChatDiffLine[]|null);

            /** ChatDiffFile patch */
            patch?: (string|null);

            /** ChatDiffFile truncated */
            truncated?: (boolean|null);
        }

        /** Represents a ChatDiffFile. */
        class ChatDiffFile implements IChatDiffFile {

            /**
             * Constructs a new ChatDiffFile.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.app.IChatDiffFile);

            /** ChatDiffFile id. */
            public id: string;

            /** ChatDiffFile path. */
            public path: string;

            /** ChatDiffFile added. */
            public added: number;

            /** ChatDiffFile removed. */
            public removed: number;

            /** ChatDiffFile lines. */
            public lines: muster.app.IChatDiffLine[];

            /** ChatDiffFile patch. */
            public patch: string;

            /** ChatDiffFile truncated. */
            public truncated: boolean;

            /**
             * Creates a new ChatDiffFile instance using the specified properties.
             * @param [properties] Properties to set
             * @returns ChatDiffFile instance
             */
            public static create(properties?: muster.app.IChatDiffFile): muster.app.ChatDiffFile;

            /**
             * Encodes the specified ChatDiffFile message. Does not implicitly {@link muster.app.ChatDiffFile.verify|verify} messages.
             * @param message ChatDiffFile message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.app.IChatDiffFile, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a ChatDiffFile message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns ChatDiffFile
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.app.ChatDiffFile;

            /**
             * Gets the default type url for ChatDiffFile
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a ChatDiffLine. */
        interface IChatDiffLine {

            /** ChatDiffLine kind */
            kind?: (string|null);

            /** ChatDiffLine text */
            text?: (string|null);

            /** ChatDiffLine oldLine */
            oldLine?: (number|null);

            /** ChatDiffLine newLine */
            newLine?: (number|null);
        }

        /** Represents a ChatDiffLine. */
        class ChatDiffLine implements IChatDiffLine {

            /**
             * Constructs a new ChatDiffLine.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.app.IChatDiffLine);

            /** ChatDiffLine kind. */
            public kind: string;

            /** ChatDiffLine text. */
            public text: string;

            /** ChatDiffLine oldLine. */
            public oldLine: number;

            /** ChatDiffLine newLine. */
            public newLine: number;

            /**
             * Creates a new ChatDiffLine instance using the specified properties.
             * @param [properties] Properties to set
             * @returns ChatDiffLine instance
             */
            public static create(properties?: muster.app.IChatDiffLine): muster.app.ChatDiffLine;

            /**
             * Encodes the specified ChatDiffLine message. Does not implicitly {@link muster.app.ChatDiffLine.verify|verify} messages.
             * @param message ChatDiffLine message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.app.IChatDiffLine, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a ChatDiffLine message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns ChatDiffLine
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.app.ChatDiffLine;

            /**
             * Gets the default type url for ChatDiffLine
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a TerminalsView. */
        interface ITerminalsView {

            /** TerminalsView deviceId */
            deviceId?: (string|null);

            /** TerminalsView sessions */
            sessions?: (muster.v1.ITerminalSession[]|null);

            /** TerminalsView sshAvailable */
            sshAvailable?: (boolean|null);

            /** TerminalsView loading */
            loading?: (boolean|null);

            /** TerminalsView errors */
            errors?: (string[]|null);

            /** TerminalsView canCreate */
            canCreate?: (boolean|null);

            /** TerminalsView sources */
            sources?: (muster.v1.ITerminalSource[]|null);
        }

        /** Represents a TerminalsView. */
        class TerminalsView implements ITerminalsView {

            /**
             * Constructs a new TerminalsView.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.app.ITerminalsView);

            /** TerminalsView deviceId. */
            public deviceId: string;

            /** TerminalsView sessions. */
            public sessions: muster.v1.ITerminalSession[];

            /** TerminalsView sshAvailable. */
            public sshAvailable: boolean;

            /** TerminalsView loading. */
            public loading: boolean;

            /** TerminalsView errors. */
            public errors: string[];

            /** TerminalsView canCreate. */
            public canCreate: boolean;

            /** TerminalsView sources. */
            public sources: muster.v1.ITerminalSource[];

            /**
             * Creates a new TerminalsView instance using the specified properties.
             * @param [properties] Properties to set
             * @returns TerminalsView instance
             */
            public static create(properties?: muster.app.ITerminalsView): muster.app.TerminalsView;

            /**
             * Encodes the specified TerminalsView message. Does not implicitly {@link muster.app.TerminalsView.verify|verify} messages.
             * @param message TerminalsView message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.app.ITerminalsView, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a TerminalsView message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns TerminalsView
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.app.TerminalsView;

            /**
             * Gets the default type url for TerminalsView
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a TerminalView. */
        interface ITerminalView {

            /** TerminalView deviceId */
            deviceId?: (string|null);

            /** TerminalView session */
            session?: (string|null);

            /** TerminalView source */
            source?: (string|null);

            /** TerminalView cols */
            cols?: (number|null);

            /** TerminalView rows */
            rows?: (number|null);

            /** TerminalView lines */
            lines?: (muster.v1.ITerminalLine[]|null);

            /** TerminalView cursorX */
            cursorX?: (number|null);

            /** TerminalView cursorY */
            cursorY?: (number|null);

            /** TerminalView ended */
            ended?: (boolean|null);

            /** TerminalView status */
            status?: (string|null);

            /** TerminalView resizable */
            resizable?: (boolean|null);

            /** TerminalView canPaste */
            canPaste?: (boolean|null);

            /** TerminalView desktopAttachCommand */
            desktopAttachCommand?: (string|null);

            /** TerminalView iterm2AttachCommand */
            iterm2AttachCommand?: (string|null);

            /** TerminalView nativeSources */
            nativeSources?: (muster.v1.ITerminalSource[]|null);
        }

        /** Represents a TerminalView. */
        class TerminalView implements ITerminalView {

            /**
             * Constructs a new TerminalView.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.app.ITerminalView);

            /** TerminalView deviceId. */
            public deviceId: string;

            /** TerminalView session. */
            public session: string;

            /** TerminalView source. */
            public source: string;

            /** TerminalView cols. */
            public cols: number;

            /** TerminalView rows. */
            public rows: number;

            /** TerminalView lines. */
            public lines: muster.v1.ITerminalLine[];

            /** TerminalView cursorX. */
            public cursorX: number;

            /** TerminalView cursorY. */
            public cursorY: number;

            /** TerminalView ended. */
            public ended: boolean;

            /** TerminalView status. */
            public status: string;

            /** TerminalView resizable. */
            public resizable: boolean;

            /** TerminalView canPaste. */
            public canPaste: boolean;

            /** TerminalView desktopAttachCommand. */
            public desktopAttachCommand: string;

            /** TerminalView iterm2AttachCommand. */
            public iterm2AttachCommand: string;

            /** TerminalView nativeSources. */
            public nativeSources: muster.v1.ITerminalSource[];

            /**
             * Creates a new TerminalView instance using the specified properties.
             * @param [properties] Properties to set
             * @returns TerminalView instance
             */
            public static create(properties?: muster.app.ITerminalView): muster.app.TerminalView;

            /**
             * Encodes the specified TerminalView message. Does not implicitly {@link muster.app.TerminalView.verify|verify} messages.
             * @param message TerminalView message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.app.ITerminalView, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a TerminalView message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns TerminalView
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.app.TerminalView;

            /**
             * Gets the default type url for TerminalView
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a FilesView. */
        interface IFilesView {

            /** FilesView deviceId */
            deviceId?: (string|null);

            /** FilesView session */
            session?: (string|null);

            /** FilesView path */
            path?: (string|null);

            /** FilesView entries */
            entries?: (muster.v1.IFileEntry[]|null);

            /** FilesView loading */
            loading?: (boolean|null);
        }

        /** Represents a FilesView. */
        class FilesView implements IFilesView {

            /**
             * Constructs a new FilesView.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.app.IFilesView);

            /** FilesView deviceId. */
            public deviceId: string;

            /** FilesView session. */
            public session: string;

            /** FilesView path. */
            public path: string;

            /** FilesView entries. */
            public entries: muster.v1.IFileEntry[];

            /** FilesView loading. */
            public loading: boolean;

            /**
             * Creates a new FilesView instance using the specified properties.
             * @param [properties] Properties to set
             * @returns FilesView instance
             */
            public static create(properties?: muster.app.IFilesView): muster.app.FilesView;

            /**
             * Encodes the specified FilesView message. Does not implicitly {@link muster.app.FilesView.verify|verify} messages.
             * @param message FilesView message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.app.IFilesView, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a FilesView message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns FilesView
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.app.FilesView;

            /**
             * Gets the default type url for FilesView
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a FileView. */
        interface IFileView {

            /** FileView deviceId */
            deviceId?: (string|null);

            /** FileView session */
            session?: (string|null);

            /** FileView path */
            path?: (string|null);

            /** FileView content */
            content?: (Uint8Array|null);

            /** FileView truncated */
            truncated?: (boolean|null);

            /** FileView loading */
            loading?: (boolean|null);

            /** FileView isDiff */
            isDiff?: (boolean|null);

            /** FileView currentPath */
            currentPath?: (string|null);

            /** FileView error */
            error?: (string|null);
        }

        /** Represents a FileView. */
        class FileView implements IFileView {

            /**
             * Constructs a new FileView.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.app.IFileView);

            /** FileView deviceId. */
            public deviceId: string;

            /** FileView session. */
            public session: string;

            /** FileView path. */
            public path: string;

            /** FileView content. */
            public content: Uint8Array;

            /** FileView truncated. */
            public truncated: boolean;

            /** FileView loading. */
            public loading: boolean;

            /** FileView isDiff. */
            public isDiff: boolean;

            /** FileView currentPath. */
            public currentPath: string;

            /** FileView error. */
            public error: string;

            /**
             * Creates a new FileView instance using the specified properties.
             * @param [properties] Properties to set
             * @returns FileView instance
             */
            public static create(properties?: muster.app.IFileView): muster.app.FileView;

            /**
             * Encodes the specified FileView message. Does not implicitly {@link muster.app.FileView.verify|verify} messages.
             * @param message FileView message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.app.IFileView, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a FileView message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns FileView
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.app.FileView;

            /**
             * Gets the default type url for FileView
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a GitView. */
        interface IGitView {

            /** GitView deviceId */
            deviceId?: (string|null);

            /** GitView session */
            session?: (string|null);

            /** GitView commits */
            commits?: (muster.v1.IGitCommit[]|null);

            /** GitView diffCommit */
            diffCommit?: (string|null);

            /** GitView diff */
            diff?: (string|null);

            /** GitView loading */
            loading?: (boolean|null);
        }

        /** Represents a GitView. */
        class GitView implements IGitView {

            /**
             * Constructs a new GitView.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.app.IGitView);

            /** GitView deviceId. */
            public deviceId: string;

            /** GitView session. */
            public session: string;

            /** GitView commits. */
            public commits: muster.v1.IGitCommit[];

            /** GitView diffCommit. */
            public diffCommit: string;

            /** GitView diff. */
            public diff: string;

            /** GitView loading. */
            public loading: boolean;

            /**
             * Creates a new GitView instance using the specified properties.
             * @param [properties] Properties to set
             * @returns GitView instance
             */
            public static create(properties?: muster.app.IGitView): muster.app.GitView;

            /**
             * Encodes the specified GitView message. Does not implicitly {@link muster.app.GitView.verify|verify} messages.
             * @param message GitView message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.app.IGitView, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a GitView message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns GitView
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.app.GitView;

            /**
             * Gets the default type url for GitView
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of an OpenBrowser. */
        interface IOpenBrowser {

            /** OpenBrowser device */
            device?: (string|null);

            /** OpenBrowser draft */
            draft?: (string|null);
        }

        /** Represents an OpenBrowser. */
        class OpenBrowser implements IOpenBrowser {

            /**
             * Constructs a new OpenBrowser.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.app.IOpenBrowser);

            /** OpenBrowser device. */
            public device: string;

            /** OpenBrowser draft. */
            public draft: string;

            /**
             * Creates a new OpenBrowser instance using the specified properties.
             * @param [properties] Properties to set
             * @returns OpenBrowser instance
             */
            public static create(properties?: muster.app.IOpenBrowser): muster.app.OpenBrowser;

            /**
             * Encodes the specified OpenBrowser message. Does not implicitly {@link muster.app.OpenBrowser.verify|verify} messages.
             * @param message OpenBrowser message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.app.IOpenBrowser, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes an OpenBrowser message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns OpenBrowser
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.app.OpenBrowser;

            /**
             * Gets the default type url for OpenBrowser
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a BrowserSetup. */
        interface IBrowserSetup {

            /** BrowserSetup install */
            install?: (boolean|null);
        }

        /** Represents a BrowserSetup. */
        class BrowserSetup implements IBrowserSetup {

            /**
             * Constructs a new BrowserSetup.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.app.IBrowserSetup);

            /** BrowserSetup install. */
            public install: boolean;

            /**
             * Creates a new BrowserSetup instance using the specified properties.
             * @param [properties] Properties to set
             * @returns BrowserSetup instance
             */
            public static create(properties?: muster.app.IBrowserSetup): muster.app.BrowserSetup;

            /**
             * Encodes the specified BrowserSetup message. Does not implicitly {@link muster.app.BrowserSetup.verify|verify} messages.
             * @param message BrowserSetup message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.app.IBrowserSetup, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a BrowserSetup message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns BrowserSetup
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.app.BrowserSetup;

            /**
             * Gets the default type url for BrowserSetup
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a BrowserRun. */
        interface IBrowserRun {

            /** BrowserRun json */
            json?: (string|null);
        }

        /** Represents a BrowserRun. */
        class BrowserRun implements IBrowserRun {

            /**
             * Constructs a new BrowserRun.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.app.IBrowserRun);

            /** BrowserRun json. */
            public json: string;

            /**
             * Creates a new BrowserRun instance using the specified properties.
             * @param [properties] Properties to set
             * @returns BrowserRun instance
             */
            public static create(properties?: muster.app.IBrowserRun): muster.app.BrowserRun;

            /**
             * Encodes the specified BrowserRun message. Does not implicitly {@link muster.app.BrowserRun.verify|verify} messages.
             * @param message BrowserRun message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.app.IBrowserRun, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a BrowserRun message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns BrowserRun
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.app.BrowserRun;

            /**
             * Gets the default type url for BrowserRun
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a BrowserView. */
        interface IBrowserView {

            /** BrowserView device */
            device?: (string|null);

            /** BrowserView local */
            local?: (boolean|null);

            /** BrowserView draft */
            draft?: (string|null);

            /** BrowserView result */
            result?: (string|null);

            /** BrowserView status */
            status?: (muster.v1.IBrowserStatus|null);

            /** BrowserView busy */
            busy?: (boolean|null);
        }

        /** Represents a BrowserView. */
        class BrowserView implements IBrowserView {

            /**
             * Constructs a new BrowserView.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.app.IBrowserView);

            /** BrowserView device. */
            public device: string;

            /** BrowserView local. */
            public local: boolean;

            /** BrowserView draft. */
            public draft: string;

            /** BrowserView result. */
            public result: string;

            /** BrowserView status. */
            public status?: (muster.v1.IBrowserStatus|null);

            /** BrowserView busy. */
            public busy: boolean;

            /**
             * Creates a new BrowserView instance using the specified properties.
             * @param [properties] Properties to set
             * @returns BrowserView instance
             */
            public static create(properties?: muster.app.IBrowserView): muster.app.BrowserView;

            /**
             * Encodes the specified BrowserView message. Does not implicitly {@link muster.app.BrowserView.verify|verify} messages.
             * @param message BrowserView message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.app.IBrowserView, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a BrowserView message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns BrowserView
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.app.BrowserView;

            /**
             * Gets the default type url for BrowserView
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a TraceSpan. */
        interface ITraceSpan {

            /** TraceSpan id */
            id?: (string|null);

            /** TraceSpan kind */
            kind?: (string|null);

            /** TraceSpan title */
            title?: (string|null);

            /** TraceSpan detail */
            detail?: (string|null);

            /** TraceSpan status */
            status?: (string|null);

            /** TraceSpan startedUnix */
            startedUnix?: (number|Long|null);

            /** TraceSpan endedUnix */
            endedUnix?: (number|Long|null);

            /** TraceSpan durationKnown */
            durationKnown?: (boolean|null);

            /** TraceSpan durationSeconds */
            durationSeconds?: (number|Long|null);
        }

        /** Represents a TraceSpan. */
        class TraceSpan implements ITraceSpan {

            /**
             * Constructs a new TraceSpan.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.app.ITraceSpan);

            /** TraceSpan id. */
            public id: string;

            /** TraceSpan kind. */
            public kind: string;

            /** TraceSpan title. */
            public title: string;

            /** TraceSpan detail. */
            public detail: string;

            /** TraceSpan status. */
            public status: string;

            /** TraceSpan startedUnix. */
            public startedUnix: (number|Long);

            /** TraceSpan endedUnix. */
            public endedUnix: (number|Long);

            /** TraceSpan durationKnown. */
            public durationKnown: boolean;

            /** TraceSpan durationSeconds. */
            public durationSeconds: (number|Long);

            /**
             * Creates a new TraceSpan instance using the specified properties.
             * @param [properties] Properties to set
             * @returns TraceSpan instance
             */
            public static create(properties?: muster.app.ITraceSpan): muster.app.TraceSpan;

            /**
             * Encodes the specified TraceSpan message. Does not implicitly {@link muster.app.TraceSpan.verify|verify} messages.
             * @param message TraceSpan message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.app.ITraceSpan, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a TraceSpan message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns TraceSpan
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.app.TraceSpan;

            /**
             * Gets the default type url for TraceSpan
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a SessionTerminalAction. */
        interface ISessionTerminalAction {

            /** SessionTerminalAction action */
            action?: (muster.app.SessionTerminalAction.Action|null);
        }

        /** Represents a SessionTerminalAction. */
        class SessionTerminalAction implements ISessionTerminalAction {

            /**
             * Constructs a new SessionTerminalAction.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.app.ISessionTerminalAction);

            /** SessionTerminalAction action. */
            public action: muster.app.SessionTerminalAction.Action;

            /**
             * Creates a new SessionTerminalAction instance using the specified properties.
             * @param [properties] Properties to set
             * @returns SessionTerminalAction instance
             */
            public static create(properties?: muster.app.ISessionTerminalAction): muster.app.SessionTerminalAction;

            /**
             * Encodes the specified SessionTerminalAction message. Does not implicitly {@link muster.app.SessionTerminalAction.verify|verify} messages.
             * @param message SessionTerminalAction message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.app.ISessionTerminalAction, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a SessionTerminalAction message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns SessionTerminalAction
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.app.SessionTerminalAction;

            /**
             * Gets the default type url for SessionTerminalAction
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        namespace SessionTerminalAction {

            /** Action enum. */
            enum Action {
                START = 0,
                END = 1,
                RESTART = 2,
                DETACH = 3
            }
        }

        /** Properties of a SessionTerminalView. */
        interface ISessionTerminalView {

            /** SessionTerminalView available */
            available?: (boolean|null);

            /** SessionTerminalView cwd */
            cwd?: (string|null);

            /** SessionTerminalView busy */
            busy?: (boolean|null);

            /** SessionTerminalView error */
            error?: (string|null);

            /** SessionTerminalView attached */
            attached?: (boolean|null);

            /** SessionTerminalView canEnd */
            canEnd?: (boolean|null);

            /** SessionTerminalView canRestart */
            canRestart?: (boolean|null);
        }

        /** Represents a SessionTerminalView. */
        class SessionTerminalView implements ISessionTerminalView {

            /**
             * Constructs a new SessionTerminalView.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.app.ISessionTerminalView);

            /** SessionTerminalView available. */
            public available: boolean;

            /** SessionTerminalView cwd. */
            public cwd: string;

            /** SessionTerminalView busy. */
            public busy: boolean;

            /** SessionTerminalView error. */
            public error: string;

            /** SessionTerminalView attached. */
            public attached: boolean;

            /** SessionTerminalView canEnd. */
            public canEnd: boolean;

            /** SessionTerminalView canRestart. */
            public canRestart: boolean;

            /**
             * Creates a new SessionTerminalView instance using the specified properties.
             * @param [properties] Properties to set
             * @returns SessionTerminalView instance
             */
            public static create(properties?: muster.app.ISessionTerminalView): muster.app.SessionTerminalView;

            /**
             * Encodes the specified SessionTerminalView message. Does not implicitly {@link muster.app.SessionTerminalView.verify|verify} messages.
             * @param message SessionTerminalView message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.app.ISessionTerminalView, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a SessionTerminalView message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns SessionTerminalView
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.app.SessionTerminalView;

            /**
             * Gets the default type url for SessionTerminalView
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a MessagePart. */
        interface IMessagePart {

            /** MessagePart kind */
            kind?: (string|null);

            /** MessagePart text */
            text?: (string|null);

            /** MessagePart language */
            language?: (string|null);

            /** MessagePart commandId */
            commandId?: (string|null);
        }

        /** Represents a MessagePart. */
        class MessagePart implements IMessagePart {

            /**
             * Constructs a new MessagePart.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.app.IMessagePart);

            /** MessagePart kind. */
            public kind: string;

            /** MessagePart text. */
            public text: string;

            /** MessagePart language. */
            public language: string;

            /** MessagePart commandId. */
            public commandId: string;

            /**
             * Creates a new MessagePart instance using the specified properties.
             * @param [properties] Properties to set
             * @returns MessagePart instance
             */
            public static create(properties?: muster.app.IMessagePart): muster.app.MessagePart;

            /**
             * Encodes the specified MessagePart message. Does not implicitly {@link muster.app.MessagePart.verify|verify} messages.
             * @param message MessagePart message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.app.IMessagePart, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a MessagePart message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns MessagePart
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.app.MessagePart;

            /**
             * Gets the default type url for MessagePart
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a CodeAction. */
        interface ICodeAction {

            /** CodeAction action */
            action?: (muster.app.CodeAction.Action|null);

            /** CodeAction id */
            id?: (string|null);

            /** CodeAction autoReturn */
            autoReturn?: (boolean|null);
        }

        /** Represents a CodeAction. */
        class CodeAction implements ICodeAction {

            /**
             * Constructs a new CodeAction.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.app.ICodeAction);

            /** CodeAction action. */
            public action: muster.app.CodeAction.Action;

            /** CodeAction id. */
            public id: string;

            /** CodeAction autoReturn. */
            public autoReturn: boolean;

            /**
             * Creates a new CodeAction instance using the specified properties.
             * @param [properties] Properties to set
             * @returns CodeAction instance
             */
            public static create(properties?: muster.app.ICodeAction): muster.app.CodeAction;

            /**
             * Encodes the specified CodeAction message. Does not implicitly {@link muster.app.CodeAction.verify|verify} messages.
             * @param message CodeAction message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.app.ICodeAction, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a CodeAction message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns CodeAction
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.app.CodeAction;

            /**
             * Gets the default type url for CodeAction
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        namespace CodeAction {

            /** Action enum. */
            enum Action {
                PREVIEW = 0,
                RUN = 1,
                DISMISS = 2,
                RETURN = 3,
                REFRESH = 4
            }
        }

        /** Properties of a CodePreview. */
        interface ICodePreview {

            /** CodePreview id */
            id?: (string|null);

            /** CodePreview code */
            code?: (string|null);

            /** CodePreview language */
            language?: (string|null);

            /** CodePreview cwd */
            cwd?: (string|null);

            /** CodePreview deviceName */
            deviceName?: (string|null);
        }

        /** Represents a CodePreview. */
        class CodePreview implements ICodePreview {

            /**
             * Constructs a new CodePreview.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.app.ICodePreview);

            /** CodePreview id. */
            public id: string;

            /** CodePreview code. */
            public code: string;

            /** CodePreview language. */
            public language: string;

            /** CodePreview cwd. */
            public cwd: string;

            /** CodePreview deviceName. */
            public deviceName: string;

            /**
             * Creates a new CodePreview instance using the specified properties.
             * @param [properties] Properties to set
             * @returns CodePreview instance
             */
            public static create(properties?: muster.app.ICodePreview): muster.app.CodePreview;

            /**
             * Encodes the specified CodePreview message. Does not implicitly {@link muster.app.CodePreview.verify|verify} messages.
             * @param message CodePreview message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.app.ICodePreview, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a CodePreview message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns CodePreview
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.app.CodePreview;

            /**
             * Gets the default type url for CodePreview
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a CodeRunView. */
        interface ICodeRunView {

            /** CodeRunView run */
            run?: (muster.v1.ITerminalRun|null);

            /** CodeRunView delivery */
            delivery?: (string|null);

            /** CodeRunView error */
            error?: (string|null);

            /** CodeRunView canReturn */
            canReturn?: (boolean|null);
        }

        /** Represents a CodeRunView. */
        class CodeRunView implements ICodeRunView {

            /**
             * Constructs a new CodeRunView.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.app.ICodeRunView);

            /** CodeRunView run. */
            public run?: (muster.v1.ITerminalRun|null);

            /** CodeRunView delivery. */
            public delivery: string;

            /** CodeRunView error. */
            public error: string;

            /** CodeRunView canReturn. */
            public canReturn: boolean;

            /**
             * Creates a new CodeRunView instance using the specified properties.
             * @param [properties] Properties to set
             * @returns CodeRunView instance
             */
            public static create(properties?: muster.app.ICodeRunView): muster.app.CodeRunView;

            /**
             * Encodes the specified CodeRunView message. Does not implicitly {@link muster.app.CodeRunView.verify|verify} messages.
             * @param message CodeRunView message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.app.ICodeRunView, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a CodeRunView message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns CodeRunView
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.app.CodeRunView;

            /**
             * Gets the default type url for CodeRunView
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }

        /** Properties of a PendingAgentSend. */
        interface IPendingAgentSend {

            /** PendingAgentSend request */
            request?: (muster.v1.IAgentSendRequest|null);

            /** PendingAgentSend draft */
            draft?: (muster.app.IChatDraft|null);

            /** PendingAgentSend draftId */
            draftId?: (string|null);

            /** PendingAgentSend dismissedToken */
            dismissedToken?: (string|null);
        }

        /** Represents a PendingAgentSend. */
        class PendingAgentSend implements IPendingAgentSend {

            /**
             * Constructs a new PendingAgentSend.
             * @param [properties] Properties to set
             */
            constructor(properties?: muster.app.IPendingAgentSend);

            /** PendingAgentSend request. */
            public request?: (muster.v1.IAgentSendRequest|null);

            /** PendingAgentSend draft. */
            public draft?: (muster.app.IChatDraft|null);

            /** PendingAgentSend draftId. */
            public draftId: string;

            /** PendingAgentSend dismissedToken. */
            public dismissedToken: string;

            /**
             * Creates a new PendingAgentSend instance using the specified properties.
             * @param [properties] Properties to set
             * @returns PendingAgentSend instance
             */
            public static create(properties?: muster.app.IPendingAgentSend): muster.app.PendingAgentSend;

            /**
             * Encodes the specified PendingAgentSend message. Does not implicitly {@link muster.app.PendingAgentSend.verify|verify} messages.
             * @param message PendingAgentSend message or plain object to encode
             * @param [writer] Writer to encode to
             * @returns Writer
             */
            public static encode(message: muster.app.IPendingAgentSend, writer?: $protobuf.Writer): $protobuf.Writer;

            /**
             * Decodes a PendingAgentSend message from the specified reader or buffer.
             * @param reader Reader or buffer to decode from
             * @param [length] Message length if known beforehand
             * @returns PendingAgentSend
             * @throws {Error} If the payload is not a reader or valid buffer
             * @throws {$protobuf.util.ProtocolError} If required fields are missing
             */
            public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): muster.app.PendingAgentSend;

            /**
             * Gets the default type url for PendingAgentSend
             * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
             * @returns The default type url
             */
            public static getTypeUrl(typeUrlPrefix?: string): string;
        }
    }
}
